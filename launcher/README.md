# Launcher

The one screen everyone sees. Not started yet; this is the brief for Phase 3.

## Must do

- Run full screen as the first window of the `player` session.
- Ask for a nickname once; write it into Mumble and every game's config.
- Show the games from [`../games/`](../games/) that are installed and fit this seat's tier.
- Per game: **Play**, **Host LAN game**, **Join**.
- A Steam tile that opens Steam for a personal or café login.
- Status bar: voice status, push-to-talk key, **End session**.
- Work with keyboard, mouse and gamepad.

## Must be testable without a person

Every action also works from the command line, so Claude and CI can test games, not just configs:

```bash
arcade launch ut99 --profile C --screenshot-after 20s
arcade end-session
```

## Runs on top of

`umu-launcher` with a pinned GE-Proton, native ports, `gamescope` (integer scaling for low-resolution games) and `gamemode`.

## Stack

Whatever the contributors know best; Python with Qt (PySide6) or Tauri both fit.
