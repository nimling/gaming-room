# Launcher design

The launcher follows **Fluent 2**. It must look modern and be easy to use with keyboard, mouse or gamepad. The screen is [`app/ui/index.html`](app/ui/index.html). Open it in a browser and it is a clickable prototype on sample data; inside the app it shows the real seat.

## Ease of use comes first

- **Two inputs to play.** Pick a game, press Play. Enter on a game, or A on a gamepad, also plays.
- **No questions at launch.** Settings come from the seat's tier and the manifest. The app has two dialogs: the End session confirmation and the IP addresses list, which the player opens.
- **Join where people are.** Every game shows the LAN games running now. Join goes to the busiest open game in one click.
- **State is always visible.** A running game, people in voice, who is talking, the seat's tier.
- **Getting out is one button.** End session is always in the status bar.
- **Spatial navigation.** Arrow keys and the d-pad move focus to the nearest control in that direction. B or Esc goes back. Every control has the Fluent focus visual.

## Fluent 2 rules

| Topic | Rule |
| --- | --- |
| Materials | Mica for the base, tinted by the selected game's art. Acrylic for flyouts, dialogs and toasts. |
| Type | Fluent ramp: Caption 12/16, Body 14/20, Subtitle 20/28, Title 28/36, Title Large 40/52, Display 68/92. Semibold for headings. |
| Font | Segoe UI Variable is not licensed for Linux. Use Open Sans (same designer), or Selawik as a fallback. |
| Shape | 4 px corners on controls, 8 px on cards, flyouts and dialogs. Spacing on a 4 px grid. |
| Color | Fluent dark is the default; light theme also works. One accent color. Semantic colors only for state. |
| Motion | Fluent curves: `cubic-bezier(0,0,0,1)` to enter, `cubic-bezier(1,0,1,1)` to exit; 167, 250 and 333 ms. Respect reduced motion. |
| Focus | 2 px outer and 1 px inner stroke on every focusable control. |
| Controls | Accent button for the one main action per screen. Standard and subtle buttons for the rest. Pivot (selector bar) for filters. |

## What the prototype shows

1. Home: hero for the selected game with Play, Host LAN game and Join; LAN games on the right; game shelf below.
2. Launch: the steps the launcher does for you (tier settings, your name, skip intro, start) and the time since Play.
3. Running: Resume or Quit; the tile shows Running.
4. Voice: Lobby flyout with who is talking; mute; push to talk on the mouse side button or V.
5. End session: one confirmation, then the seat reset, then the nickname screen.
6. IP addresses: a status bar button opens a list of every seat and the server. Pick one to copy its address.
7. Seat flyout: the tier, with a prototype switch to see how A, B and C change the tuning.

## Stack recommendation

**Tauri with this HTML and CSS as the UI.** The Fluent look needs real blur (`backdrop-filter`), and the prototype already has it. The Rust side runs `umu-run`, `gamescope` and `gamemoderun`, reads the manifests and talks to Mumble. Fluent UI Web Components (MIT) can replace the hand-made controls.

To check before we commit to it: WebKitGTK blur speed on a Tier C seat under KWin Wayland. If it is too slow, Qt Quick with its own blur is the fallback.

## Art

The prototype draws placeholder art from code. Real art goes in each game's manifest as a path, using art we have the right to use (our own screenshots or the publisher's press kit).
