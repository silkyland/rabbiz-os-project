#!/usr/bin/env python3
"""Summarize a bounded gnss_ipc capture without printing payload data."""
import argparse
import re
import sys
from functools import reduce
from operator import xor

LIMIT = 65536
NMEA_SENTENCE = re.compile(rb"\$([A-Z]{2}[A-Z0-9]{3},[ -~]{0,73}?)\*([0-9A-Fa-f]{2})\r?\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args()
    data = sys.stdin.buffer.read(LIMIT)
    count = sum(
        reduce(xor, body, 0) == int(checksum, 16)
        for body, checksum in NMEA_SENTENCE.findall(data)
    )
    print(f"bytes_sampled={len(data)} valid_nmea_sentences={count}")
    if not data:
        print("No payload observed")
        return 1
    print("NMEA sentences observed" if count else "Payload format unclassified")
    return 0


if __name__ == "__main__":
    sys.exit(main())
