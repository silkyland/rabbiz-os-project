# A21s Kepler GNSS bring-up checkpoints

Stock A217FXXU8DVF6 enables `EXYNOS_GNSS_IF` and `GNSS_PMUCAL`. Its
Android init configuration assigns permissions to `/dev/gnss_ipc` and
starts Samsung's GNSS daemon. Static strings in that binary describe
Kepler code load and startup via BCMD ioctls and NMEA encoding in userspace.
They provide no live sample of the kernel endpoint.
[Stock dump](https://github.com/leddaz-dump-stash/samsung_a21s_dump)

## Power, reset and transport

1. The first BCMD request powers on an offline Kepler or releases a held
   reset. Exynos3830 PMUCAL sequences assert and release the GNSS block
   and poll status. The controller waits for SW_INIT, requests TZPC
   access through the vendor SMC and configures GNSS-to-AP BAAW.
   Bring-up must confirm each result before advancing to firmware commands.
   [Kepler controller](https://github.com/samsungexynos850/android_kernel_samsung_exynos850/blob/b8c71b7aecece9c67f0e840a6b074573e8041d20/drivers/soc/samsung/gnssif/gnss_keplerctl_device.c)
2. The r06 overlay places the mailbox at `0x11980000` (SPI 53).
   The BCMD doorbells use index 0 and data IPC uses index 2 in each
   direction. BCMD carries control words in mailbox registers. Data IPC
   uses the shared rings with mailbox IRQ notification.
   [A21s r06 overlay](https://github.com/samsungexynos850/android_kernel_samsung_exynos850/blob/b8c71b7aecece9c67f0e840a6b074573e8041d20/arch/arm64/boot/dts/samsung/a21s/a21s_eur_open_w00_r06.dts)
3. The base DT reserves 6 MiB at `0xee000000`. IPC starts at offset
   `0x380000` and spans `0x80000`. The vendor driver assigns the first
   `0x40000` bytes to RX and the next `0x3ff80` to TX. The final
   `0x80` bytes hold shared registers, including ring indices.
   Verify the live memory map before enabling the reservation.
   [Shared-memory implementation](https://github.com/samsungexynos850/android_kernel_samsung_exynos850/blob/b8c71b7aecece9c67f0e840a6b074573e8041d20/drivers/soc/samsung/gnssif/gnss_link_device_shmem.c)

## What the character device delivers

The shared-memory frames use a 12-byte Exynos link header, 16-bit
`0xabcd` start word, frame length at byte offset 6, and alignment
padding to a four-byte boundary. The driver trims the padding and
removes the header before queueing each payload. Short reads return
part of a payload and requeue the remainder. An empty read can mean an
empty queue. The endpoint therefore exposes GNSS IPC payload bytes.
The inner Kepler message format and any direct NMEA output remain unverified.
[Character-device implementation](https://github.com/samsungexynos850/android_kernel_samsung_exynos850/blob/b8c71b7aecece9c67f0e840a6b074573e8041d20/drivers/soc/samsung/gnssif/gnss_io_device.c)

## Test checkpoints

Once a port exposes the endpoint, first log SW_INIT and a successful
BCMD reply. Then check that an RX IRQ has a valid ring head and that the
driver consumes a complete frame. Use a single reader under the shared
device lock. On the stock driver, a second reader can consume data from
the same queue and closing a reader can purge the queue. An idle boot
without the firmware startup sequence is inconclusive.

Pipe a bounded payload capture to
[the host classifier](../../../../scripts/check_a21s_gnss_payload.py).
It reports byte and checksum-valid NMEA sentence counts without
printing payloads or coordinates. A nonempty sample without a valid
NMEA sentence stays unclassified. Keep a first satellite fix as a
separate checkpoint after transport and message decoding.
