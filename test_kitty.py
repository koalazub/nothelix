#!/usr/bin/env python3
"""Test Kitty graphics protocol Unicode placeholder rendering"""
import sys
import base64
import struct
import zlib

PLACEHOLDER_CHAR = '\U0010EEEE'
ROW_DIACRITICS = ['\u0305', '\u030D', '\u030E', '\u0310', '\u0312', '\u033D', '\u033E', '\u033F']

def create_png(width, height, r, g, b):
    def png_chunk(chunk_type, data):
        chunk_len = struct.pack('>I', len(data))
        chunk_crc = struct.pack('>I', zlib.crc32(chunk_type + data) & 0xffffffff)
        return chunk_len + chunk_type + data + chunk_crc
    signature = b'\x89PNG\r\n\x1a\n'
    ihdr_data = struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0)
    ihdr = png_chunk(b'IHDR', ihdr_data)
    raw_data = b''
    for y in range(height):
        raw_data += b'\x00'
        for x in range(width):
            raw_data += bytes([r, g, b])
    compressed = zlib.compress(raw_data)
    idat = png_chunk(b'IDAT', compressed)
    iend = png_chunk(b'IEND', b'')
    return signature + ihdr + idat + iend

def transmit_image(image_id, b64_data):
    CHUNK_SIZE = 4096
    chunks = [b64_data[i:i+CHUNK_SIZE] for i in range(0, len(b64_data), CHUNK_SIZE)]
    for i, chunk in enumerate(chunks):
        is_first = i == 0
        is_last = i == len(chunks) - 1
        if is_first:
            m = 0 if is_last else 1
            sys.stdout.write(f"\x1b_Gf=100,t=d,a=t,i={image_id},q=2,m={m};{chunk}\x1b\\")
        else:
            m = 0 if is_last else 1
            sys.stdout.write(f"\x1b_Gm={m};{chunk}\x1b\\")

def placeholder_cell(image_id, row, col):
    r = (image_id >> 16) & 0xFF
    g = (image_id >> 8) & 0xFF
    b = image_id & 0xFF
    if col == 0:
        return f"\x1b[38;2;{r};{g};{b}m{PLACEHOLDER_CHAR}{ROW_DIACRITICS[row]}\x1b[39m"
    else:
        return f"\x1b[38;2;{r};{g};{b}m{PLACEHOLDER_CHAR}\x1b[39m"

image_id = 999
cols, rows = 10, 5
png_bytes = create_png(50, 50, 255, 0, 0)
b64_data = base64.b64encode(png_bytes).decode('ascii')

print("Transmitting...", file=sys.stderr)
transmit_image(image_id, b64_data)
sys.stdout.write(f"\x1b_Ga=p,U=1,i={image_id},c={cols},r={rows},q=2;\x1b\\")

print("Text before image:")
for row in range(rows):
    print("".join(placeholder_cell(image_id, row, col) for col in range(cols)))
print("Text after image")
sys.stdout.flush()
print("\nIf you see a red square, it works!", file=sys.stderr)
