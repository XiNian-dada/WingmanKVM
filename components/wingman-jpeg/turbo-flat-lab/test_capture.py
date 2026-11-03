"""JPEG boundary tests; no devices, SSH, processes, or filesystem writes."""
import unittest
from live_capture import jpeg_end

SOF = b"\xff\xc0\x00\x0b\x08\x00\x01\x00\x01\x01\x01\x11\x00"
SOS = b"\xff\xda\x00\x08\x01\x01\x00\x00\x3f\x00"


class BoundaryTests(unittest.TestCase):
    def test_payload_stuffing_restart_and_embedded_eoi(self):
        for payload in [b"xyz", b"a\xff\x00\xd9z", b"a\xff\xd0z"]:
            with self.subTest(payload=payload):
                data = b"\xff\xd8\xff\xe1\x00\x06a\xff\xd9b" + SOF + SOS + payload + b"\xff\xd9TAIL"
                end, meta = jpeg_end(data)
                self.assertEqual(data[end:], b"TAIL")
                self.assertEqual(meta["width"], 1)

    def test_incomplete_and_missing_scan(self):
        for data in [b"", b"\xff\xd8", b"\xff\xd8" + SOF + SOS + b"abc",
                     b"\xff\xd8\xff\xe0\x00\x09a", b"\xff\xd8" + SOF + b"\xff\xd9"]:
            with self.subTest(data=data), self.assertRaises(ValueError):
                jpeg_end(data)

    def test_multiple_scans(self):
        data = b"\xff\xd8" + SOF + SOS + b"abc" + SOS + b"def\xff\xd9TAIL"
        end, _ = jpeg_end(data)
        self.assertEqual(data[end:], b"TAIL")


if __name__ == "__main__":
    unittest.main()
