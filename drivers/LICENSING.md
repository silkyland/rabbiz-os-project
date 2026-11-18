# Licensing of `drivers/`

Everything in this directory comes from, or is derived from, upstream sources (Samsung
Electronics open-source kernel releases and the Linux kernel). It is **not** covered by the
repository's Apache-2.0 licence.

- Each source file keeps the licence stated in its own header (`SPDX-License-Identifier` or
  licence comment); for kernel modules this is GPL-2.0 or GPL-2.0-only / compatible.
- Do not remove or alter existing copyright/licence headers.
- New files you add here must carry an `SPDX-License-Identifier` line that is compatible with the
  Linux kernel (`GPL-2.0-only` unless the module is deliberately dual-licensed).
- Do not add vendor firmware blobs or binaries. Firmware is loaded from the device's own
  partitions at runtime and is not redistributed by this project.

Known gap: some files here still lack an SPDX header. Adding the correct upstream header to
them (never guessing) is a welcome docs-lane contribution.
