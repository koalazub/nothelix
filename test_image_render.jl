# Quick test script to verify image rendering
using Steel

# Load the library
require("~/.config/helix/plugins/nothelix.scm")

# Test protocol detection
println("Testing protocol detection...")
protocol = detect-graphics-protocol()
println("Detected protocol: ", protocol)

# Test capabilities
caps = protocol-capabilities(protocol)
println("Capabilities: ", caps)

# Create a tiny test PNG (1x1 red pixel)
# PNG signature + IHDR + IDAT + IEND
test_png_hex = "89504e470d0a1a0a0000000d49484452000000010000000108020000009077531a0000000c49444154789c6260f8cf0000000301000162f6d40000000049454e44ae426082"

# Convert hex to bytes and then to base64
# For now, just use a pre-encoded base64 string
test_b64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGNg+M8AAAQAAQE="

println("Test base64 length: ", string-length(test_b64))

# Test rendering
println("Testing render-b64-for-protocol...")
escape_seq = render-b64-for-protocol(test_b64, protocol, 1)

if (string-starts-with? escape_seq "ERROR:")
  (println "ERROR: " escape_seq)
else
  (println "✓ Generated escape sequence")
  (println "Sequence length: " (string-length escape_seq))
  (println "First 50 chars: " (substring escape_seq 0 50))
