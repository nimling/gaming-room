# Runbook: Worms Armageddon

Bought on Steam (or GOG), copied once to the gaming server, and every seat copies it from there. It runs through Proton; Team17's 3.8 update made it run well there.

```
Steam (one seat) ──copy the game folder──▶ server /srv/games/worms ──rsync (read-only)──▶ each seat /games/classic/worms
```

## Licences

Decide the licence model before it goes to all seats: one copy per seat, or the per-player ownership check Nimtech uses for MW2 and Warcraft III.

## 1. Get the game onto the server

1. On a seat, sign in to Steam with the account that owns the game, install **Worms Armageddon**, and start it once so Steam finishes the install. Steam puts it in `/games/steam/steamapps/common/Worms Armageddon`.
2. From the server, copy the whole folder into the share (the folder exists after `server.yml`):

   ```bash
   rsync -a --info=progress2 orchestrator@<seat>:"/games/steam/steamapps/common/Worms Armageddon/" /srv/games/worms/
   chmod -R u+rwX,go+rX /srv/games/worms
   ```

3. Uninstall it from Steam on that seat if it should not stay there.

## 2. Seats

Seats copy the game at their next pull once `/srv/games/worms/WA.exe` exists. The launcher starts it with `proton-view`, which builds a writable view of the read-only folder in the player's home, because the game saves teams and settings next to itself. End session throws that view away.

## LAN play

In the game: Network → LAN. Hosts announce themselves by UDP broadcast on ports 17010 to 17012; the seats' firewall allows the gaming VLAN.
