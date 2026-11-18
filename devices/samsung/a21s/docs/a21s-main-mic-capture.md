# A21s main microphone capture probe

The SM-A217F Android 12 dump (A217FXXU8DVF6) expands
`recording-mic → media-mic → dev-main-mic + route-ap-record`.
The [stock mixer config](https://github.com/leddaz-dump-stash/samsung_a21s_dump/blob/a21snsxx-user-12-SP1A.210812.016-A217FXXU8DVF6-release-keys/vendor/etc/mixer_paths.xml)
selects the analog MIC1 input. The [repo mixer reference](../../../../docs/audio/mixer_paths_a21s.xml)
selects `dev-dual-mic` for the same alias. The probe follows the stock
single-mic controls and leaves gain settings at their current values.

| Stage | Mixer controls |
|---|---|
| AUD3004X MIC1 via IN1L | Both ADC input selectors choose AMIC_L. MIC1 feeds the left mixer. The right MIC1 mixer switch stays off. The route enables AMIC bias and MIC1. |
| Codec serial capture → ABOX NSRC4 | `ABOX NSRC4` selects `UAIF0`. |
| NSRC4 → WDMA4 → virtual capture | `ABOX SIFM4` selects `WDMA`. `ABOX VPCMIN_DAI0_A` and `ABOX VPCMIN_DAI2_A` select `WDMA4`. |

The stock route identifies the virtual input selectors. The expected
PCM candidates are `calliope_10` and `calliope_12`, based on the
[Calliope topology notes](HANDOFF.md) and the A21s PCM numbering convention.
Whether each selector maps to the expected candidate still needs a live
capture check.

## Run after the DE device session ends

Use the shared lock for mixer changes and recording. Keep playback and
other captures idle and choose a private location for the WAV. On the
device, use `arecord -l` and `/proc/asound/card*/id` to identify the
ABOX card. Start with the capture PCM listed as `calliope_10` (expected
device 110). If the first PCM is clean but silent, repeat separately on
`calliope_12` (expected device 112). Record a quiet interval and then
speak close to the lower microphone.

Run [the probe](../../../../scripts/abox_main_mic_capture.sh) under the
shared lock. It accepts the card number, PCM device number, and a new
absolute WAV path. It checks PCM identity, snapshots only its ten mixer
controls, restores them on exit and requests six seconds of 48 kHz stereo
S16 capture with 960-frame periods and a 3,840-frame buffer (15,360 bytes).
The recording is written locally on the A21s.

Compare the two channels for silence, clipping, and speech that matches
the quiet/speech intervals. Check the verbose ALSA output for the
negotiated PCM parameters and the kernel log for new DSP errors,
especially data aborts and buffer exhaustion. A successful command or
nonempty WAV alone does not establish that the physical microphone works.
