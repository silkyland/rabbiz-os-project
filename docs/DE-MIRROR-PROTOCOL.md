# A native screen-mirroring protocol for RabbizOS (design proposal)

Status: **design, nothing is built**. Lane: `de`. Written 2026-11-18. It follows the owner's direction of
2026-11-18: mirroring should use **our own protocol, written in Rust**, with emphasis on **adaptiveness,
performance and ecosystem**; VNC stays supported for outside tools but is not our protocol.

## 1. Why not just keep VNC

Facts from the A21s today ([finding](../devices/samsung/a21s/findings/2026-11-18-de-wayvnc-unauthenticated-localhost.md)):
`a21s-wayvnc` offers no authentication and listens on usb0 and loopback. VNC (RFB) also has no notion of the things a phone
needs: thermal and battery budgets, a trusted-device pairing, our input model (touch, pointer, keyboard, controller),
rotation and form factor changes, or a clipboard policy. Its encodings are not adaptive to the link or the CPU.
This is a guess about performance, not a measurement: nobody has measured wayvnc's frame rate or CPU on the A21s yet.

## 2. Goals and non-goals

Goals
- **Adaptive:** frame size, rate, quality and codec follow the measured link, the encoder's cost, battery and
  temperature. The phone must stay cool; mirroring must degrade, not stall the compositor.
- **Fast on a weak SoC:** send only what changed (damage), reuse the capture path, no extra copies.
- **Secure by default:** nothing listens without authentication; a new computer needs a one-time approval on the
  phone; a locked phone shows nothing (decided by the owner: captures are blank while locked).
- **An ecosystem, not one app:** a small, versioned, documented wire format with a Rust crate for servers and clients
  and an independent Swift client for the macOS companion (Rabbiz Mirror); others can follow the spec.
- One capture pipeline shared with screenshots and screen recording.

Non-goals (v1): remote file transfer, multi-user sessions, internet relaying, audio output from the computer to the phone.

## 3. Working name and crates

Proposed name **Rabbiz Link** (the owner picks; "Mirror" stays the macOS app's name).

| Crate | Content | Depends on |
|---|---|---|
| `rabbiz-link-proto` | wire types, framing, version negotiation, no I/O, `forbid(unsafe_code)`, fuzzed parser | nothing |
| `rabbiz-link-server` | runs on the device: capture, encode, adaptive controller, auth | proto, the compositor's capture API |
| `rabbiz-link-client` | reference client library (Rust), used by tests and tools | proto |
| Swift package | macOS client written from the spec, not a binding | the written spec |

## 4. Architecture on the device

```
compositor frame ──► capture (screencopy today; an in-process path later, no allow-list round trip)
        │                    blank when the lock screen is showing
        ▼
   damage tracker ──► adaptive controller ──► encoder ──► transport ──► Mac
        ▲                    ▲   ▲   ▲
        │                    │   │   └── RTT, throughput, queue depth (from the transport)
   input channel ◄── events  │   └────── CPU/encoder load, battery, thermal zones (TMU)
                             └────────── user setting: quality / battery / auto
```

- **Capture:** `wlr-screencopy` (shm) is what exists. It reads client shm buffers only; GPU/dmabuf client content shows as the
  clear colour, which is a known gap that a native path inside the compositor should remove.
- **Encoding, in order of preference, chosen at run time:**
  1. changed tiles, lossless or near-lossless, compressed with a fast general codec (zstd or lz4) when the link is wide (USB);
  2. a video codec (H.264 first) when the link is narrow or the content is moving: a hardware encoder only if the kernel lane
     confirms one is usable on the Exynos 850 (**not known**), otherwise software x264 at low-latency settings with a
     CPU cap, because the A21s is a small CPU.
  The controller may switch between them per frame region.
- **Display dimming (owner request):** while a mirror is connected the phone's own screen dims. The dim is a compositor
  overlay drawn on the physical output only, so it does not appear in the captures. A visible mirror indicator is shown
  the whole time.

## 5. Transport and security (proposal)

- **Transport:** QUIC. Pros: one connection with independent streams (video must not block input), built-in encryption,
  loss tolerance, and macOS can speak QUIC natively through Network.framework, so the companion needs no extra stack;
  on the device the Rust `quinn` crate fits the project's language. Cons: UDP over the USB network must be allowed and
  tested; QUIC is heavier than a plain TCP socket. Fallback if it disappoints: TCP + TLS 1.3 with the same framing.
- **Pairing:** first connection from a new computer shows a confirmation on the phone (the device must be unlocked) with a
  short code displayed on both sides; both sides then pin each other's public keys. Later connections need no prompt
  while the phone is unlocked. Revocation lives in Settings.
- **No unauthenticated listener, no loopback listener.** The server accepts only the pinned peers and only on the interfaces
  the user enabled (USB by default).
- **Lock rule:** blank frames while the lock screen shows, and input is ignored then.
- **Allow-list:** the server is a privileged compositor client under the policy in
  [DE-PRIVILEGED-PROTOCOLS.md](DE-PRIVILEGED-PROTOCOLS.md); it does not use VNC.

## 6. Wire sketch (to be turned into a spec)

Streams: `control` (hello, version, capabilities, pairing, quality hints, rotation/size changes, keep-alive), `video`
(frame headers with a sequence number, region list, codec id, timestamp, payloads), `input` (touch with multi-touch slots,
pointer, key, scroll, controller state), `clipboard` (subject to a per-session switch, off by default), `audio` (later).
All integers little-endian, every message length-prefixed, an explicit protocol version with a downgrade path, unknown
message types ignored, hard size limits on everything the peer controls. A conformance test suite lives with the spec.

## 7. Milestones

| | Goal | Evidence |
|---|---|---|
| L0 | spec draft + `rabbiz-link-proto` with round-trip and fuzz tests | host tests |
| L1 | server and Rust client on a host over loopback with a synthetic frame source | host tests, throughput numbers |
| L2 | on the A21s over USB: pairing, blank on lock, dimming, tiles + zstd | device log, CPU and temperature numbers |
| L3 | the macOS client in Swift against the same server | a recording from the Mac |
| L4 | adaptive controller and the video codec path (hardware if available) | before/after tables for CPU, bitrate, latency |
| L5 | input channel (touch, pointer, keyboard, controller), clipboard, audio | on-device checks |

Screen recording reuses L1/L2's capture and encoder (see the recording package in the DE brief).

## 8. Open questions for the owner

1. Name: Rabbiz Link, or another?
2. v1 scope: video + input only, or clipboard and audio from the start?
3. QUIC as the primary transport, with TCP+TLS only as a fallback?
4. Is a software-only encoder acceptable on the A21s if no hardware codec exists, even if it limits the frame rate?
5. Should the existing Rabbiz Mirror app (Swift, VNC today) be rewritten against the spec, or should a second app appear?
