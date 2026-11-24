# Playback settings

Settings has **Video & playback** and **Audio & subtitles** pages. Quality and Direct Play
are saved on this television; language preferences are saved to the signed-in Jellyfin user on
the server and are shared with that user's other Jellyfin clients.

## Quality and Direct Play

Default quality uses the same ladder and saved preference as the player's More menu. Settings
changes apply to the next playback; the player menu retains its existing current-playback behavior.

Direct Play defaults to **Auto**, which uses device capabilities and normal routing.
**Disabled** excludes original-file playback while allowing a remux or transcode.
**Force Direct Play (advanced)** attempts original playback through supported engine paths,
overrides the saved quality and disables automatic remux/transcode fallback. It does not add
codec implementations of its own: TrueHD and the other formats the app converts to AC-3 play under
Force exactly as under Auto. It cannot guarantee that the TV accepts a stream.

Entering Force requires an explicit acknowledgement, initially focused on Cancel. The warning
names missing sound, incorrect pictures, freezes, crashes and the possibility that the app
cannot recover gracefully. It instructs the user to enable Force only if they know how to
restart the app and return to Auto. An enabled-state warning remains visible in Settings;
the quality controls explain that the saved quality is overridden. Returning to Auto or
Disabled needs no confirmation.

Audio passthrough remains the television's Digital Sound Output setting.

## Lossless audio to receiver

**Playback → Lossless audio to receiver** (off by default, saved on this television; persisted as
`dts_passthrough`) sends DTS, DTS-HD and Dolby TrueHD to an external receiver unconverted. Many sets
have no DTS decoder (a webOS 6 C1 refuses a `DTS` Load with `type=18 num=601`) and none plays TrueHD
from an app. The setting is used only while `com.webos.service.audio/getSoundOutput` reports
`external_arc` or `external_optical`: on the TV's speakers the bursts would be noise.

Three routes, chosen per title, best first:

| Route | Carries | Needs |
|---|---|---|
| DD+ carrier, HBR | DTS-HD MA and DTS:X at full rate, TrueHD and Atmos (MAT) | **Atmos, TrueHD and DTS:X (root)** installed |
| DD+ carrier, 192 kHz | DTS-HD MA up to ~6 Mb/s per frame (larger frames send their core) | a receiver LG passes Dolby Digital Plus through to |
| PCM core | the DTS 5.1 core | any receiver on ARC/eARC or optical |

Without HBR, TrueHD keeps the on-TV AC-3 conversion (`ff_audio_convert.rs`).

**The DD+ carrier** (`player/es_carrier.rs`). LG's own audio library (liblxa, with libkadaptor and
libpf) runs inside this process, and for a Dolby Digital Plus title it packs each E-AC-3 frame into
an IEC 61937 burst and writes it into LG's ES renderer (`ioctl(fd, 0x40285207, arg)`, word 8 = 1),
from where LG's eARC pass-through sends it untouched. The app declares a DTS or TrueHD title as
`AC3 PLUS`, feeds a silent 384-byte E-AC-3 frame every 32 ms on contiguous timestamps, and packs the
real track into IEC 61937 bursts: DTS-HD as type IV (three 512-sample frames per carrier frame),
TrueHD as MAT (type 22, a port of FFmpeg 9.0's `spdif` muxer, matched byte for byte in a test).
`src/es_swap.c` exports `ioctl` from the executable, so libkadaptor's call lands there first and
each ES write is pointed at the next queued chunk. LG keeps timing, A/V sync, pause and seek exactly
as for a DD+ title. A write the renderer refuses (`-1`, buffer full) is retried by liblxa with the
same chunk. If LG makes no ES write by the 75th picture, its DD+ route is not in pass-through on
this output: the carrier is switched off for the app run and the title reloads at the same position
on the PCM core or the AC-3 converter.

**HBR** (8 channels at 192 kHz, 4x the stereo carrier) is what TrueHD and full-rate DTS-HD need. LG
never opens it for an app, so on rooted TVs **Atmos, TrueHD and DTS:X (root)** installs the
`earc_route` kernel module (`vendor/earc_route/README.md`, GPL-2.0-only, shipped beside the app in
`hbr/`). The row runs `hbr/nativejelly-hbr.sh` as root through Homebrew Channel's `exec` service: it
copies the module beside Homebrew Channel's boot hooks (`/var/lib/webosbrew/init.d`) so it loads at
every start, loads it through dts_restore_plus's loader (which refuses kernels the build does not
match; the module also checks every instruction it hooks), sets its HBR parameters and puts its
lease node in each Native Jelly sandbox (`/var/palm/jail/<app id>/dev/earc_route`). During a
playback the app writes `2` to that node; while it is held the module makes LG configure the eARC
link as 8 channels at 192 kHz, and the app writes 4x larger bursts. The node is opened only when
`/sys/dev/char/<major>:<minor>/uevent` names `earc_route`, because its number changes per load.
Tested on an LG C1 (webOS 6, firmware 03.53.45) with a Sony STR-DN1080: DTS-HD MA, DTS:X and
TrueHD Atmos with picture, surviving a reboot. The row shows only while lossless audio is on; it
reads the installed state from the lease node, and choosing **Not installed** removes the boot hook
(the module stays loaded, switched off, until the next reboot).

## Jellyfin user preferences

**Audio & subtitles** edits the four playback fields of the signed-in user's Jellyfin
`UserConfiguration` (`jf/preferences.rs`, issue #41):

| Row | Field | Options |
|---|---|---|
| Preferred audio language | `AudioLanguagePreference` | No preference, or one of the server's languages |
| Subtitles | `SubtitleMode` | As the file marks them (`Default`), When audio isn't in the subtitle language (`Smart`), Always, Only forced subtitles (`OnlyForced`), Off (`None`) |
| Subtitle language | `SubtitleLanguagePreference` | No preference, or one of the server's languages |
| Default audio track | `PlayDefaultAudioTrack` | Play the file's default track, or Prefer my audio language |

The values live on the server, per user, so every Jellyfin client of that user shares them, and
nothing is stored on the television. Playback reads them from `/Users/Me` at each start
(`Jf::language_prefs`), so a save applies from the next playback.

A load reads `GET /Users/Me` (its `Id` must be the signed-in user, or the answer is refused as
stale) and `GET /Localization/Cultures`. The languages are the server's list, keyed by
`ThreeLetterISOLanguageName` (`eng`, `fre`): the codes the server stores and reports on a stream,
and the list jellyfin-web's settings page offers. A stored code the server no longer lists stays
visible and selected.

`POST /Users/Configuration?userId=` replaces the WHOLE configuration, so a save reads
`/Users/Me` again, changes only the field picked, and sends every other field back as the server
gave it. If the four fields no longer match what the page showed (another client changed them),
the save is refused as stale and the page reloads instead of overwriting a change nobody saw. A
pick of the value already checked sends nothing. Confirmed values stay authoritative if a save
fails; the page offers Retry. A request captured under one sign-in cannot be saved under another.

Jellyfin has no forced-subtitle preference beside the mode, so the Plex-era *Forced subtitles*
row is gone; `OnlyForced` is a Subtitles mode.

## Verification boundary

Host tests drive the load and save against a fake server (`jf/preferences_tests.rs`). A real
user's preferences must not be changed as part of verification: use a throwaway user on a test
server.

Host tests cannot establish DTS sound, synchronization, resource allocation or DTS-HD core
acceptance. Those require the synthetic DTS fixture and server-backed playback on the television.
A muted device run proves pipeline behavior, not audible output.

Auto keeps the measured DTS channel ceiling: DTS-HD sources reporting more channels than that
ceiling (including 7.1 on the six-channel dev set), or no usable channel count, still convert on
the server. Stripping an HD extension alone does not prove the remaining core fits the decoder;
this change does not advertise all DTS-HD streams as directly playable.

### Implementation verification — 2026-09-27

- `make check` passed: default and hostsim Rust suites, feature checks, native helpers and the
  Python harness. After the final initial-load focus correction, all ten composed Settings
  tests passed, including load/retry focus, save focus, and Force confirmation/cancellation.
- Shipping-feature check (`--no-default-features`, `CARGO_INCREMENTAL=0`), ARM cross-build,
  ELF assertions, and the supported firmware loader matrix passed. No new FFI was introduced.
- All three committed replay anchors were freshly recorded after the state-shape change and
  passed both Targets and Resolve: six runs, every difference counter zero. The alphabet adds
  only source-backed Direct Play enum values and fixed Settings copy.
- Simulator: full account language picker, initial focus, regional-code save to the mock,
  warning with Cancel selected, and persistent Force/quality-override presentation inspected.
- LG webOS 4.10.2 debug install: Force warning and persistent enabled-state copy captured and
  inspected. A server-backed H.264/AC3 case passed while Force was persisted, using the managed
  Guest test identity. Direct Play was then returned to Auto.
- Native H.264/DTS 5.1 synthetic playback passed all pipeline assertions, including declaration,
  video binding, clock progress, displayed frames and in-place seeking.
- DTS-HD MA 5.1 core playback passed the same assertions using a temporary interoperability
  fixture. The public source sample's truncated tail was removed before looping its valid first
  3.4 seconds; every one of the resulting 5,532 audio packets contained a complete core. The
  malformed packet guard remains intact. The sample media is not distributed in this repository.

The television remained muted with its panel off. These results do not claim audible output
verification, and no real Plex account language preferences were written. The throwaway-account
PMS propagation measurement was never made, and is moot since #41: the page no longer writes plex.tv.
