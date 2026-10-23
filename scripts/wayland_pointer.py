#!/usr/bin/env python3
"""A virtual pointer on a wlroots compositor (sway), as a touchpad or mouse
the compositor treats as its own: `zwlr_virtual_pointer_v1`, spoken over
the Wayland socket in plain Python (no pywayland). It moves, and scrolls as
fingers do (axis source finger, then axis stop as they lift), which neither
xdotool nor an X11-backed compositor can.

    pointer = VirtualPointer(os.environ["XDG_RUNTIME_DIR"], "wayland-1")
    pointer.move_to(400, 300, 1400, 900)
    pointer.swipe(dy=20, frames=12)   # two fingers, 120 Hz, then lifted
"""
import os
import socket
import struct
import time

# Requests and events by opcode, from the protocols' XML.
DISPLAY_SYNC, DISPLAY_GET_REGISTRY = 0, 1
REGISTRY_BIND = 0
MANAGER_CREATE = 0
MOTION_ABSOLUTE, AXIS, FRAME, AXIS_SOURCE, AXIS_STOP = 1, 3, 4, 5, 6
FINGER = 1
VERTICAL, HORIZONTAL = 0, 1


def _string(text):
    data = text.encode() + b"\0"
    return struct.pack("<I", len(data)) + data + b"\0" * (-len(data) % 4)


def _fixed(value):
    return struct.pack("<i", int(round(value * 256)))


class VirtualPointer:
    def __init__(self, runtime_dir, display):
        self.socket = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.socket.connect(os.path.join(runtime_dir, display))
        self.buffer = b""
        self.next_id = 2
        registry = self._new_id()
        self._send(1, DISPLAY_GET_REGISTRY, struct.pack("<I", registry))
        done = self._new_id()
        self._send(1, DISPLAY_SYNC, struct.pack("<I", done))
        manager = None
        while True:
            sender, opcode, body = self._event()
            if sender == registry and opcode == 0:
                name, = struct.unpack_from("<I", body)
                length, = struct.unpack_from("<I", body, 4)
                interface = body[8:8 + length - 1].decode()
                version, = struct.unpack_from("<I", body, 8 + length + (-length % 4))
                if interface == "zwlr_virtual_pointer_manager_v1":
                    manager = self._new_id()
                    self._send(registry, REGISTRY_BIND, struct.pack("<I", name) + _string(interface)
                               + struct.pack("<II", min(version, 2), manager))
            elif sender == done:
                break
            elif sender == 1 and opcode == 0:
                raise RuntimeError("Wayland protocol error: " + body.hex())
        if manager is None:
            raise RuntimeError("the compositor has no zwlr_virtual_pointer_manager_v1")
        self.pointer = self._new_id()
        # A null seat: the compositor's default.
        self._send(manager, MANAGER_CREATE, struct.pack("<II", 0, self.pointer))

    def _new_id(self):
        self.next_id += 1
        return self.next_id - 1

    def _send(self, sender, opcode, body=b""):
        self.socket.sendall(struct.pack("<II", sender, (8 + len(body)) << 16 | opcode) + body)

    def _event(self):
        while len(self.buffer) < 8 or len(self.buffer) < (struct.unpack_from("<I", self.buffer, 4)[0] >> 16):
            data = self.socket.recv(4096)
            if not data:
                raise RuntimeError("the compositor closed the connection")
            self.buffer += data
        sender, word = struct.unpack_from("<II", self.buffer)
        size = word >> 16
        body, self.buffer = self.buffer[8:size], self.buffer[size:]
        return sender, word & 0xFFFF, body

    @staticmethod
    def _now():
        return int(time.monotonic() * 1000) & 0xFFFFFFFF

    def move_to(self, x, y, width, height):
        """To (`x`, `y`) of the `width` by `height` output layout."""
        self._send(self.pointer, MOTION_ABSOLUTE, struct.pack("<IIIII", self._now(), x, y, width, height))
        self._send(self.pointer, FRAME)

    def swipe(self, dy=0., dx=0., frames=12, hz=120.):
        """Two fingers scrolling `dx`, `dy` (Wayland's sense: positive
        scrolls down) each of `frames` frames, then lifting: axis stop."""
        for _ in range(frames):
            self._send(self.pointer, AXIS_SOURCE, struct.pack("<I", FINGER))
            if dy:
                self._send(self.pointer, AXIS, struct.pack("<II", self._now(), VERTICAL) + _fixed(dy))
            if dx:
                self._send(self.pointer, AXIS, struct.pack("<II", self._now(), HORIZONTAL) + _fixed(dx))
            self._send(self.pointer, FRAME)
            time.sleep(1. / hz)
        # The lift: a stop on each axis that scrolled (wlroots holds every
        # axis event of a frame to one source).
        self._send(self.pointer, AXIS_SOURCE, struct.pack("<I", FINGER))
        for axis, moved in ((VERTICAL, dy), (HORIZONTAL, dx)):
            if moved:
                self._send(self.pointer, AXIS_STOP, struct.pack("<II", self._now(), axis))
        self._send(self.pointer, FRAME)

    def close(self):
        self.socket.close()
