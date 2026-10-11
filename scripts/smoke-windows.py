#!/usr/bin/env python3
"""Exercise Windows discovery, Lantern's own sessions and safe delivery against a harmless agent.

The fixture is laid out like an npm-installed Claude Code (a `claude.cmd` shim running `node
...\\node_modules\\@anthropic-ai\\claude-code\\cli.js`). It writes a short transcript where Claude
Code would, records its arguments and any line typed into it, and waits. No real AI agent is
started. An agent started in a console of its own must not be typed into; one Lantern started in
its own session must be, until that session ends.
"""
import json
import os
import pathlib
import queue
import subprocess
import tempfile
import sys
import threading
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
ENGINE = ROOT / "build" / "windows" / "lantern-engine.exe"
if not ENGINE.is_file():
    raise SystemExit("Build the Windows app first: pwsh scripts/build-windows.ps1 -Debug")
# --bus: also start the session on October Bus (needs scripts/build-bus-windows.ps1).
BUS = "--bus" in sys.argv

FIXTURE = r"""
const fs = require('fs'), path = require('path');
const slug = process.cwd().replace(/[^A-Za-z0-9]/g, '-');
const dir = path.join(process.env.USERPROFILE, '.claude', 'projects', slug);
fs.mkdirSync(dir, { recursive: true });
const at = new Date().toISOString();
const lines = [
  { type: 'user', timestamp: at, message: { role: 'user', content: 'Is Lantern on Windows working?' } },
  { type: 'assistant', timestamp: at, message: { id: 'm1', stop_reason: 'end_turn',
    content: [{ type: 'text', text: 'Windows discovery works' }] } },
];
fs.writeFileSync(path.join(dir, '00000000-0000-4000-8000-000000000001.jsonl'),
  lines.map((l) => JSON.stringify(l)).join('\n') + '\n');
const out = process.env.LANTERN_SMOKE_OUT;
if (out && process.cwd().endsWith('project')) {
  fs.writeFileSync(path.join(out, 'args.json'), JSON.stringify(process.argv.slice(2)));
  process.stdin.on('data', (d) => fs.writeFileSync(path.join(out, 'reply.txt'), d));
}
setInterval(() => {}, 1000);
"""

SHIM = (
    "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\nSETLOCAL\r\nCALL :find_dp0\r\n"
    "SET \"_prog=node\"\r\n"
    "endLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & \"%_prog%\"  "
    "\"%dp0%\\node_modules\\@anthropic-ai\\claude-code\\cli.js\" %*\r\n"
)

with tempfile.TemporaryDirectory(prefix="lantern smoke-", ignore_cleanup_errors=True) as folder:
    # The long form of the path (no RUNNER~1): that is what the agent's working directory reports.
    folder = pathlib.Path(os.path.realpath(folder))
    bin_dir = folder / "bin"
    script = bin_dir / "node_modules" / "@anthropic-ai" / "claude-code" / "cli.js"
    script.parent.mkdir(parents=True)
    script.write_text(FIXTURE)
    (bin_dir / "claude.cmd").write_text(SHIM, newline="")
    project = folder / "project"
    project.mkdir()
    env = dict(os.environ, USERPROFILE=str(folder / "home"), LOCALAPPDATA=str(folder / "data"),
               LANTERN_SMOKE_OUT=str(folder), PATH=f"{bin_dir};{os.environ['PATH']}",
               OCTOBER_BUS_RUNTIME_DIR=str(folder / "bus-runtime"), OCTOBER_BUS_DATA_DIR=str(folder / "bus-data"))
    agent_proc = subprocess.Popen(
        ["node", str(script)], cwd=folder, env=env,
        stdin=subprocess.DEVNULL, creationflags=subprocess.CREATE_NEW_CONSOLE,
    )
    proc = subprocess.Popen(
        [str(ENGINE), "serve"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=subprocess.PIPE, text=True, env=env, bufsize=1, encoding="utf-8",
    )
    events = queue.Queue()

    def reader():
        for line in proc.stdout:
            events.put(json.loads(line))
        events.put(None)

    threading.Thread(target=reader, daemon=True).start()

    def read_until(predicate, seconds=30):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            try:
                value = events.get(timeout=max(0.01, deadline - time.monotonic()))
            except queue.Empty:
                break
            if value is None:
                raise RuntimeError("engine exited: " + proc.stderr.read())
            if predicate(value):
                return value
        raise RuntimeError("engine response timed out")

    def send(request):
        proc.stdin.write(json.dumps(request) + "\n")
        proc.stdin.flush()

    def ours(snapshot):
        return [a for a in snapshot["agents"] if a.get("cwd") == str(folder)]

    try:
        read_until(lambda v: v.get("type") == "hello")
        # Found, and its transcript matched (the last message comes from the session file).
        snapshot = read_until(
            lambda v: v.get("type") == "snapshot"
            and any(a.get("lastMessage") == "Windows discovery works" for a in ours(v))
        )
        agent = ours(snapshot)[0]
        print("agent:", json.dumps({k: agent.get(k) for k in ("kind", "pid", "tty", "state", "host", "route")}))
        assert agent["kind"] == "claude", agent
        assert agent["tty"].startswith("con:"), agent
        assert not agent["canReply"] and agent["route"]["via"] == "none", agent

        send({"type": "history", "requestId": "smoke-history", "agentId": agent["id"]})
        history = read_until(lambda v: v.get("requestId") == "smoke-history")
        texts = [m["text"] for m in history["messages"]]
        assert "Windows discovery works" in texts, history

        # Lantern won't type into a terminal it didn't start.
        send({"type": "reply", "requestId": "smoke-reply", "agentId": agent["id"], "text": "MUST NOT BE SENT"})
        refused = read_until(lambda v: v.get("requestId") == "smoke-reply")
        assert not refused["ok"], refused
        print("PASS: console discovery, npm-installed Claude Code, transcript and history, reply refused")

        # A session Lantern starts: the message reaches the agent intact (no shell in between),
        # replies are typed into it, and nothing is typed once it has ended.
        prompt = 'hello "Windows" & echo pwned | more %PATH% ^ \\'
        send({"type": "launch", "requestId": "smoke-launch", "kind": "claude", "cwd": str(project),
              "prompt": prompt, "background": True, "bus": BUS})
        launch = read_until(lambda v: v.get("requestId") == "smoke-launch")
        assert launch["ok"], launch
        args_file = folder / "args.json"
        for _ in range(100):
            if args_file.exists():
                break
            time.sleep(0.1)
        args = json.loads(args_file.read_text())
        # Intact, quotes, `&`, `|`, `%`, `^` and the trailing backslash included.
        assert args[-2] == "--" and args[-1].startswith(prompt) and "--session-id" in args, args
        if BUS:
            assert "--mcp-config" in args and not launch.get("warning"), (args, launch)
            scopes = subprocess.check_output([str(ENGINE.with_name("october-bus.exe")), "scope", "list"], env=env, text=True)
            assert '"lantern"' in scopes, scopes

        snapshot = read_until(
            lambda v: v.get("type") == "snapshot"
            and any(a.get("cwd") == str(project) and a["route"]["via"] == "pty" for a in v["agents"])
        )
        hosted = next(a for a in snapshot["agents"] if a.get("cwd") == str(project))
        assert hosted["canReply"] and hosted["route"]["session"] == launch["session"], hosted
        send({"type": "reply", "requestId": "smoke-reply-2", "agentId": hosted["id"],
              "text": "Windows session delivery works"})
        result = read_until(lambda v: v.get("requestId") == "smoke-reply-2")
        assert result["ok"], result
        received = folder / "reply.txt"
        for _ in range(50):
            if received.exists():
                break
            time.sleep(0.1)
        assert received.read_text().strip() == "Windows session delivery works", received.read_text()

        # End the agent: its session ends with it, and a late reply is refused.
        subprocess.run(["taskkill", "/F", "/PID", str(hosted["pid"])], check=True, capture_output=True)
        record = folder / "data" / "October Lantern" / "pty" / f"{launch['session']}.json"
        for _ in range(100):
            if not record.exists():
                break
            time.sleep(0.1)
        assert not record.exists(), "the session outlived its agent"
        send({"type": "reply", "requestId": "smoke-stale", "agentId": hosted["id"], "text": "MUST NOT BE SENT"})
        stale = read_until(lambda v: v.get("requestId") == "smoke-stale")
        assert not stale["ok"], stale
        print("PASS: Lantern session launch (no shell), reply delivery, session ends with its agent, late reply refused")
        if BUS:
            print("PASS: October Bus daemon from Lantern's copy, Lantern scope, per-session MCP configuration")
    finally:
        proc.terminate()
        proc.wait(timeout=10)
        if BUS:
            subprocess.run([str(ENGINE.with_name("october-bus.exe")), "stop"], env=env, capture_output=True, timeout=10)
        agent_proc.kill()
        agent_proc.wait(timeout=10)
