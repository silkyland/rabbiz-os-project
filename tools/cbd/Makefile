# SPDX-License-Identifier: GPL-2.0
# Build a21s-cbd for aarch64 (run on the A21s itself or via qemu).
#
#   make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu-
#   make                      # native build also works

CC ?= cc
CFLAGS ?= -O2 -Wall -Wextra
TARGET = a21s-cbd

$(TARGET): a21s-cbd.c
	$(CC) $(CFLAGS) -o $@ $<

clean:
	rm -f $(TARGET)

.PHONY: clean
