# Rave Box — Pingo

> Keywords for search: **ravebox** rave-box pingo vgalizer E480

> ThinkPad E480 repurposed as a dedicated live audio-reactive visualizer box. Sole purpose: run `vgalizer-rs` at raves. No personal data, no sensitive state, no dual-use. Hostname: **pingo**. User: **natalia**. SSH'd from desktop via `ssh natalia@<pingo-ip>`.

## Status: Phases 3–6 done, dual-display mirror live — 2026-04-08

**Current state:** pingo boots into Debian 13, PipeWire runs, vgalizer is built
(`cargo build --release` on-box from the public repo), and cage on the physical
tty runs vgalizer fullscreen mirrored across eDP-1 (laptop panel) and HDMI-A-1
(projector) simultaneously. Test ladder `vjtestshort` / `vjtestfull` /
`vjtestcage` (see `scripts/vjtest.sh`) is green end-to-end. Live for a gig.

**Next:** Phase 7 — autologin on tty1 + `exec cage -- vgalizer` on login, so the
box boots straight into the visualizer with no interactive step.

### 2026-04-08 additions
- **Dual-display mirror — cage source patched.** cage 0.2.0 has no native mirror
  mode (only `-m extend` cascading and `-m last` single-output). With both
  eDP-1 and HDMI-A-1 plugged in, stock cage splits the visuals across the two
  outputs. Fix: patched Debian's `cage 0.2.0-2` source — `src/output.c`'s
  `output_layout_add_auto()` helper was modified to pin every output to
  `(0,0)` via the existing sibling `wlr_output_layout_add(..., 0, 0)` call.
  wlroots' scene graph then renders the same scene into both scene_outputs =
  true mirror. Installed at `/usr/bin/cage`, held with `apt-mark hold cage`.
  Rebuild workspace lives at `/home/natalia/build/cage-mirror/` with a
  `install.py` one-shot installer — re-run if cage ever gets clobbered.
- **Display modes confirmed:** eDP-1 exposes only `1366x768` (E480 HD SKU, not
  FHD), HDMI-A-1 accepts `1366x768` at top of mode list. Projector is
  physically 720p but scales the 1366x768 frame internally. Mirror is
  pixel-for-pixel at the DRM layer; any scaling happens inside the projector
  hardware, not in cage or wlroots.
- **Test ladder (`scripts/vjtest.sh`)** — `vjtestshort` (60s headless),
  `vjtestfull` (2700s headless), `vjtestcage` (2700s cage on tty) all work.
  Verified: 60 fps locked, RSS stable ~110 MB, all 25 scenes cycle with
  auto mirror-mode rotation, no panics, no DRM errors. Beat detector
  triggers correctly with real audio input on the line-in.
- **cage invocation pattern:** always `cage -- bash -c "cd <dir> && exec <bin>"`.
  The `bash -c` wrapper avoids an older `cage.c:138` execvp-failure mode and
  keeps `cwd` correct for config lookup. Enshrined in `scripts/vjtest.sh`
  and the `vj` shell alias.

**Historical note below from Phase 3 bootstrap is preserved for context; the
"Next (in order)" checklist is out of date — phases 4, 5, 6 are all complete
and Phase 7 (autologin) is the next real step.**

### Environment gotchas hit during Phase 3
- **`seatd` package skipped** — the Debian seatd package ships no systemd unit; `systemctl enable seatd` fails with "Unit seatd.service does not exist". Not a blocker: systemd-logind handles seats for cage when logged in on a physical tty. Move on.
- **`sudo` was not preinstalled** — user set a root password during install (not blank), so Debian didn't auto-install sudo. Fixed via `su -` → `apt install sudo` → `usermod -aG sudo natalia` → re-login.
- **Firmware packages dropped from Phase 3 install** — `firmware-linux`, `firmware-misc-nonfree`, `firmware-iwlwifi` were deferred. WiFi already works without them. Revisit only if hardware acts flaky.
- **Multi-line pastes kept getting mangled** in SSH (continuation backslash + paste chunking caused apt to receive partial package lists). **Workaround: always paste single-line commands to pingo, or break installs into ≤5-package single-line chunks.** Don't use `\` continuation over SSH paste.

### Done
- [x] Verified Debian 13.4.0 netinst ISO on Ventoy stick (GPG + SHA512 chain), 2026-04-06
- [x] Distro + kernel + audio stack decisions locked, 2026-04-06
- [x] Ventoy stick safely ejected and booted on E480, 2026-04-07
- [x] Debian installer walkthrough complete (hostname pingo, user natalia, en_US.UTF-8, Danish keymap, Europe/Copenhagen, guided entire-disk ext4, tasksel = SSH server + standard utilities only), 2026-04-07
- [x] First boot into Debian successful, 2026-04-07
- [x] `sudo` installed, natalia in sudo group, 2026-04-07
- [x] WiFi working (not via NetworkManager — set up manually, NM installed but not yet tested), 2026-04-07
- [x] **Phase 3 packages installed** (4 chunks): `network-manager cage rtkit libasound2` + `pipewire pipewire-audio pipewire-alsa wireplumber` + `mesa-vulkan-drivers libvulkan1 vulkan-tools intel-media-va-driver libgl1-mesa-dri` + `libxkbcommon-dev libudev-dev libasound2-dev libwayland-dev intel-microcode`, 2026-04-07

### Next (in order)
- [ ] **Start here on resume**: enable services → groups → re-login. Commands:
  ```bash
  sudo systemctl enable --now NetworkManager
  sudo usermod -aG video,input,audio,render natalia
  systemctl --user enable --now pipewire pipewire.socket wireplumber
  # then exit SSH and reconnect for group membership to apply
  groups   # verify: should include video input audio render sudo
  ```
- [ ] **Phase 4 — rustup** (single command):
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
  source "$HOME/.cargo/env"
  rustc --version
  ```
- [ ] **Phase 5 — clone + build**:
  ```bash
  mkdir -p ~/repos && cd ~/repos
  git clone https://github.com/Hornfisk/vgalizer-rs.git
  cd vgalizer-rs
  cargo build --release
  ```
  (wgpu build takes a few minutes)
- [ ] **Phase 6 — test on physical tty (NOT over SSH, cage needs a seat)**:
  ```bash
  cd ~/repos/vgalizer-rs
  ./target/release/vgalizer --windowed    # sanity check
  cage -- ./target/release/vgalizer       # real fullscreen test
  ```
- [ ] Phase 7 (next session): autologin on tty1, `~/.bash_profile` exec cage, HDMI mirroring via `wlr-randr`
- [ ] Phase 8 (next session): PipeWire lockdown drop-ins (48k fixed, 1024 quantum, auto-switch off), line-in → mic fallback timer
- [ ] Phase 9 (next session): reliability locks (unattended-upgrades off, sleep masked, journal volatile, zram-swap, kernel hold)
- [ ] Codify the whole sequence into `~/repos/dotfiles/rave-box/post-install.sh` once proven working end-to-end
- [ ] Test with real projector + line-in + fallback to mic

## Debian install outcome (reference)

- Disk: `/dev/nvme0n1` — 256 GB Samsung MZVLW256HEHP
  - `#1` 1.0 GB ESP (vfat)
  - `#2` 246.6 GB ext4 → `/`
  - `#3` 8.5 GB swap
- tasksel: **SSH server + standard system utilities only** (no DE)
- Root password: blank → `natalia` is in sudo group via installer default
- Kernel: stock `linux-image-amd64` (whatever Debian 13.4 ships — 6.12-ish)

## Phase 3–6 bootstrap (in-progress, 2026-04-07)

**Revised**: cloning public `vgalizer-rs` repo and building on-box, not the .deb route. User is still iterating on visualizer. `rustup` + `build-essential` stay on pingo for fast rebuilds after `git pull`.

### Phase 3 — apt packages
```bash
sudo apt update && sudo apt install -y --no-install-recommends \
    git curl ca-certificates build-essential pkg-config \
    network-manager \
    pipewire pipewire-audio pipewire-alsa wireplumber rtkit libasound2 \
    mesa-vulkan-drivers libvulkan1 vulkan-tools intel-media-va-driver libgl1-mesa-dri \
    cage seatd \
    libxkbcommon-dev libudev-dev libasound2-dev libwayland-dev \
    intel-microcode firmware-linux firmware-misc-nonfree firmware-iwlwifi

sudo systemctl enable --now seatd NetworkManager
sudo usermod -aG video,input,audio,render natalia
systemctl --user enable --now pipewire pipewire.socket wireplumber
# log out and back in for group membership
```

### Phase 4 — rustup (apt rustc too old for wgpu/vgalizer)
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
source "$HOME/.cargo/env"
```

### Phase 5 — clone + build
```bash
mkdir -p ~/repos && cd ~/repos
git clone https://github.com/Hornfisk/vgalizer-rs.git
cd vgalizer-rs
cargo build --release
```

### Phase 6 — test (on physical tty, NOT ssh — cage needs a seat)
```bash
# Log in on physical tty1 first:
cd ~/repos/vgalizer-rs
./target/release/vgalizer --windowed       # sanity check
cage -- ./target/release/vgalizer          # fullscreen via cage
```

## What stage 2 (next session) will add

- `~/.bash_profile` on tty1: `exec cage -- ~/repos/vgalizer-rs/target/release/vgalizer`
- Autologin on tty1 via `/etc/systemd/system/getty@tty1.service.d/autologin.conf`
- HDMI mirroring — **done 2026-04-08** via a local cage 0.2.0 source patch (see
  top-of-file status section). `wlr-randr` is not used and would not work on cage
  0.2.0 anyway (no `wlr-output-management` protocol exposed).
- PipeWire lockdown drop-ins (48k fixed, 1024 quantum, auto-switch off)
- Line-in → mic fallback systemd user timer
- Reliability locks: disable unattended-upgrades, mask sleep targets, ignore lid switch, `setterm -blank 0`, iwlwifi power_save=0, journal volatile, zram-swap, `apt-mark hold linux-image-amd64`
- systemd watchdog on the cage/visualizer service
- Everything above codified into `~/repos/dotfiles/rave-box/post-install.sh` once proven working

### Installer choices (what to pick during Debian install)

| Step | Choose |
|---|---|
| Language | English |
| Location | Denmark |
| Locale | `en_US.UTF-8` |
| Keyboard | Danish |
| Hostname | **`pingo`** |
| Domain | *(blank)* |
| Root password | *(leave blank — enables sudo-for-wheel automatically)* |
| Username | **`natalia`** |
| Time zone | Europe/Copenhagen |
| Partitioning | Guided → entire disk → All files in one partition (no LVM, no encryption — sealed-purpose box) |
| Mirror | Denmark mirror |
| Popularity contest | No |
| **Software selection** | **Untick everything except `SSH server` + `standard system utilities`** |
| GRUB | Yes, install to disk |

## Core decisions

### Distro: **Debian 13 "Trixie"** stable
- Stock `linux-image-amd64`, **not** `linux-image-rt-amd64`
- Rationale: visualizer is vsync-bound to 60 Hz, so PREEMPT_RT buys nothing; stock kernel with PipeWire + rtkit covers latency needs. Rolling-release Arch rejected because an update two hours before a gig is the #1 way to brick a live box.
- Install via netinst with **only** "SSH server" + "standard system utilities" ticked. Target footprint ~1.5 GB installed, ~250 MB idle RAM.
- Kernel pinned via `apt-mark hold linux-image-amd64` before gigs.

### Compositor: **cage** (wlroots kiosk)
- One-app Wayland kiosk. Boot path: tty1 autologin → `exec cage -- vgalizer`. No desktop, no bar, no menu.
- Cage exits if vgalizer exits. Getty systemd unit has `Restart=always` → immortal visualizer.
- Laptop LCD mirrored to HDMI projector via patched cage (see top-of-file status
  section for the patch details). Framebuffer is 1366×768 (eDP-1's only native
  mode on this HD SKU); the projector receives 1366×768 and scales to its
  physical 720p internally. GPU headroom on UHD 620 is still fine — 60 fps
  locked with the current 25-effect set per `vjtestcage` runs.

### Audio stack: **PipeWire + pipewire-alsa shim** — LOCKED
- **NOT bare ALSA. NOT JACK.**
- Packages: `pipewire pipewire-audio pipewire-alsa wireplumber rtkit libasound2`
- **Do NOT install**: `pipewire-pulse`, `pipewire-jack` (no clients for them → don't run the daemons)
- Why not JACK: vgalizer's `cpal 0.15` has no `jack` feature enabled → it speaks ALSA, full stop. JACK would require rebuilding vgalizer and wouldn't help a vsync-bound visualizer anyway.
- Why not bare ALSA: the line-in → mic fallback requirement is the decisive factor. PipeWire makes it node-level routing (`wpctl set-default`) that's transparent to cpal's open stream. Bare ALSA would require teaching vgalizer to close/reopen its ALSA PCM handle on source switch, which is fragile.
- Why PipeWire wins on reliability:
  1. Fallback implementation is ~20 lines of shell, not 200
  2. Debian 13's stock audio stack IS PipeWire — going bare-ALSA means fighting the distro
  3. systemd user services with `Restart=always` = self-healing
  4. Diagnostics via `wpctl status`, `pw-top`, `pw-mon` when SSH'd in at 2am at a gig
  5. Stable node naming across reboots (USB plug/unplug won't shift enumeration)
  6. cpal + pipewire-alsa is the well-trodden 2026 path, not experimental

### PipeWire lockdown config (drop-ins, not stock)
| Setting | Value | Why |
|---|---|---|
| Sample rate | 48000 Hz, fixed | No dynamic resampling mid-stream |
| Quantum | 1024/1024/1024 (min/default/max) | ~21 ms buffer → essentially never xruns. Latency irrelevant to visualizer. |
| WirePlumber auto-switch | **disabled** | Deterministic routing. We decide when to switch sources, not WirePlumber. |
| Default source at boot | pinned to line-in via `wpctl set-default` in startup script | Line-in is primary per spec |
| RT priority | `rtkit` installed, `natalia` in `audio` group | SCHED_RR for audio thread without PREEMPT_RT kernel |
| User services | `pipewire`, `pipewire.socket`, `wireplumber` enabled, auto-restart | Self-healing |

### vgalizer-rs audio capture on pulse-free PipeWire (2026-04-06)
- vgalizer-rs now has a **native pw-cat / pw-dump capture path** in addition to the existing parec/pactl (pulse-compat) path. Wired in `src/audio/capture.rs`.
- Auto-default behavior: when `audio_device` is `"default"` or unset, vgalizer first tries `pactl` for a RUNNING `.monitor` source (pulse-compat), then falls back to `pw-dump` → first RUNNING `Audio/Sink` (pw-cat records its monitor) → then `Audio/Source`, and finally to cpal's ALSA default. **No pipewire-pulse needed on Pingo.**
- Explicit selection prefixes: `PA:<src>` / `pa:<src>` (parec) and `PW:<node>` / `pw:<node>` (pw-cat). Picker (`A` key) lists PW nodes when `pactl` is absent.
- Implication for Pingo's locked stack (no `pipewire-pulse`): vgalizer should react out-of-the-box on first boot without any extra packages — `pw-cat` ships with the base `pipewire` package. Test on first boot before adding any pulse-compat shims.
- Line-in → mic fallback design below is **still valid** as a routing-level default-source switcher; the new pw-cat path just means vgalizer can also capture explicit named nodes without going through `wpctl set-default` if we ever want per-effect routing.

### Line-in → mic fallback
- User-space systemd timer, runs every few seconds
- Uses `pw-cat -r --target <line-in-node>` piped into RMS check (`sox -n stats` or similar)
- On silence >5s: `wpctl set-default <mic-node-id>` → vgalizer transparently gets mic audio
- On line-in return: swap back
- **Critical property**: vgalizer never knows this is happening. cpal's ALSA handle stays open, PipeWire routes underneath.
- Actual node IDs resolved at install time (E480-specific: `alsa_input.pci-0000_00_1f.3.analog-stereo` etc.)

## Hardware: ThinkPad E480
- CPU: Intel i5-8250U / i7-8550U (8th gen Kaby Lake R) → `intel-ucode`
- GPU: Intel UHD 620 only, **no discrete Radeon** on this unit → `mesa-vulkan-drivers intel-media-va-driver libgl1-mesa-dri libvulkan1`
- WiFi: Intel 8265 (likely) → `iwlwifi`, firmware in `firmware-linux`
- BT: Intel (not installed — not needed on this box)
- TPM 2.0: present but unused (no LUKS — box has no sensitive data)
- Display: 1080p LCD + HDMI out to 720p projector (mirrored)

## Boot flow
1. Power on → BIOS → systemd-boot / GRUB (whatever Debian installs)
2. Stock `linux-image-amd64` + `intel-ucode`
3. systemd → autologin `natalia` on tty1
4. `~/.bash_profile` → `exec cage -- vgalizer` (fullscreen)
5. Audio fallback timer starts in parallel as user unit
6. **Total: under 10 seconds to visualizer running**

## Reliability locks for live use
- Unattended upgrades disabled (`systemctl disable --now unattended-upgrades apt-daily.timer apt-daily-upgrade.timer`)
- Kernel pinned before gigs (`apt-mark hold linux-image-amd64`)
- `sleep.target suspend.target hibernate.target hybrid-sleep.target` masked
- `HandleLidSwitch=ignore` in `/etc/systemd/logind.conf`
- `setterm -blank 0 -powersave off` on tty1
- `iwlwifi` `power_save=0` via modprobe drop-in
- Journal `Storage=volatile` (no SSD wear, no log bloat)
- zram swap instead of disk swap (`zram-tools`)
- systemd watchdog on the visualizer unit
- Second SSH key on a USB stick in the gig bag
- Ventoy stick with Debian netinst ISO as recovery in the gig bag
- systemd-boot / GRUB timeout = 0, default to stock kernel

## Build + deploy

- **Build `.deb` on desktop** (same amd64 arch, no cross-compile), stage in `~/repos/dotfiles/rave-box/dist/`
- Transfer to pingo via USB or SSH
- `dpkg -i vgalizer_0.1.0_amd64.deb` — runtime deps are just `libasound2, libvulkan1` per `Cargo.toml`'s `[package.metadata.deb]`
- Keep `rustup` + `build-essential` on pingo anyway → can rebuild on-site if needed

## SSH/tweak workflow at a gig
- SSH in from phone over wifi tether
- Edit DJ name in `vgalizer` config (hot-reloads, no restart)
- `journalctl --user -u cage -f` to watch for issues
- `wpctl status` to check audio routing

## Files (when written)
- `~/repos/dotfiles/rave-box/post-install.sh` — runs once on first boot of fresh Debian
- `~/repos/dotfiles/rave-box/build-deb.sh` — builds vgalizer .deb on desktop
- `~/repos/dotfiles/rave-box/audio-fallback.sh` — line-in → mic fallback logic
- `~/repos/dotfiles/rave-box/dist/` — built .deb artifact staging
- `~/repos/dotfiles/rave-box/README.md` — operator runbook
- `~/repos/dotfiles/rave-box/pipewire-lockdown/` — drop-in configs (sample rate, quantum, auto-switch off)

## Related notes
- `Tech/Desktop Migration Plan — Arch + Hyprland Clone.md` — the original "clone desktop to E480" plan, parked when E480 was repurposed for this
- `~/repos/vgalizer-rs/` — the visualizer itself
