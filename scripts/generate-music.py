#!/usr/bin/env python3
"""Synthesizes CodeTake's bundled background music.

The tracks are original works generated entirely by this script (no samples,
no third-party audio), and are dedicated to the public domain under CC0 1.0.
Each track loops seamlessly: note tails that run past the end wrap around to
the beginning.

Usage:
    python3 scripts/generate-music.py [output-dir]

Writes 16-bit WAV files; on macOS they are then converted to AAC (.m4a) with
the built-in `afconvert` tool. Pure Python, no dependencies; takes a few
minutes.
"""

import math
import os
import random
import shutil
import struct
import subprocess
import sys
import wave

RATE = 48_000
TAU = 2 * math.pi


def midi_hz(note: float) -> float:
    return 440.0 * 2 ** ((note - 69) / 12)


NOTE_NAMES = {"C": 0, "D": 2, "E": 4, "F": 5, "G": 7, "A": 9, "B": 11}


def note(name: str) -> int:
    """'C4' -> 60, 'F#3' -> 54, 'Bb2' -> 46."""
    base = NOTE_NAMES[name[0]]
    rest = name[1:]
    if rest.startswith("#"):
        base += 1
        rest = rest[1:]
    elif rest.startswith("b"):
        base -= 1
        rest = rest[1:]
    return base + 12 * (int(rest) + 1)


class Track:
    """A stereo loop rendered into a circular buffer."""

    def __init__(self, bpm: float, bars: int, seed: int):
        self.bpm = bpm
        self.beat = 60.0 / bpm
        self.length = int(round(bars * 4 * self.beat * RATE))
        self.left = [0.0] * self.length
        self.right = [0.0] * self.length
        self.random = random.Random(seed)

    def at(self, beat: float) -> int:
        return int(round(beat * self.beat * RATE))

    def add(self, start: int, samples, pan: float = 0.0, gain: float = 1.0):
        """Mixes mono samples in at `start`, wrapping around the loop."""
        left_gain = gain * math.cos((pan + 1) * math.pi / 4)
        right_gain = gain * math.sin((pan + 1) * math.pi / 4)
        n = self.length
        for i, s in enumerate(samples):
            j = (start + i) % n
            self.left[j] += s * left_gain
            self.right[j] += s * right_gain

    def lowpass(self, cutoff: float):
        """One-pole low-pass, run twice around the loop so it wraps cleanly."""
        a = 1 - math.exp(-TAU * cutoff / RATE)
        for channel in (self.left, self.right):
            y = 0.0
            for _ in range(2):
                for i, x in enumerate(channel):
                    y += a * (x - y)
                    channel[i] = y

    def normalize(self, peak: float = 0.8):
        top = max(max(abs(s) for s in self.left), max(abs(s) for s in self.right)) or 1.0
        scale = peak / top
        self.left = [s * scale for s in self.left]
        self.right = [s * scale for s in self.right]

    def write_wav(self, path: str):
        with wave.open(path, "wb") as out:
            out.setnchannels(2)
            out.setsampwidth(2)
            out.setframerate(RATE)
            frames = bytearray()
            for l, r in zip(self.left, self.right):
                frames += struct.pack(
                    "<hh",
                    int(max(-1.0, min(1.0, l)) * 32767),
                    int(max(-1.0, min(1.0, r)) * 32767),
                )
            out.writeframes(bytes(frames))


def envelope(i: int, attack: int, decay_rate: float, release_at: int, release: int) -> float:
    if i < attack:
        level = i / attack
    else:
        level = math.exp(-(i - attack) * decay_rate)
    if i > release_at:
        level *= max(0.0, 1 - (i - release_at) / release)
    return level


def electric_piano(freq: float, seconds: float, velocity: float = 1.0):
    """Soft Rhodes-like tone: sine with a decaying bell partial and tremolo."""
    total = int((seconds + 0.6) * RATE)
    release_at = int(seconds * RATE)
    out = []
    for i in range(total):
        t = i / RATE
        env = envelope(i, 60, 2.2 / RATE, release_at, int(0.5 * RATE))
        bell = math.exp(-t * 9) * 0.35 * math.sin(TAU * freq * 3.01 * t)
        tone = math.sin(TAU * freq * t + 0.6 * math.sin(TAU * freq * t) * math.exp(-t * 3))
        tremolo = 1 + 0.12 * math.sin(TAU * 4.5 * t)
        out.append((tone + bell) * env * tremolo * velocity * 0.22)
    return out


def pad(freq: float, seconds: float):
    """Slow, detuned additive pad."""
    total = int((seconds + 2.0) * RATE)
    attack = int(1.2 * RATE)
    release_at = int(seconds * RATE)
    release = int(2.0 * RATE)
    detunes = (0.996, 1.0, 1.004)
    harmonics = ((1, 1.0), (2, 0.35), (3, 0.15), (4, 0.07))
    out = []
    for i in range(total):
        t = i / RATE
        if i < attack:
            env = (i / attack) ** 2
        else:
            env = 1.0
        if i > release_at:
            env *= max(0.0, 1 - (i - release_at) / release)
        s = 0.0
        for d in detunes:
            for h, a in harmonics:
                s += a * math.sin(TAU * freq * d * h * t + h * d)
        out.append(s * env * 0.05)
    return out


def pluck(freq: float, seconds: float):
    """Bright, short plucked tone (a decaying mix of odd harmonics)."""
    total = int(seconds * RATE)
    out = []
    for i in range(total):
        t = i / RATE
        env = math.exp(-t * 7) * min(1.0, i / 40)
        s = (
            math.sin(TAU * freq * t)
            + 0.3 * math.sin(TAU * freq * 3 * t) * math.exp(-t * 12)
            + 0.12 * math.sin(TAU * freq * 5 * t) * math.exp(-t * 20)
        )
        out.append(s * env * 0.18)
    return out


def bass(freq: float, seconds: float):
    total = int((seconds + 0.1) * RATE)
    release_at = int(seconds * RATE)
    out = []
    for i in range(total):
        t = i / RATE
        env = envelope(i, 200, 1.2 / RATE, release_at, int(0.08 * RATE))
        s = math.sin(TAU * freq * t) + 0.25 * math.sin(TAU * freq * 2 * t)
        out.append(s * env * 0.32)
    return out


def kick():
    out = []
    for i in range(int(0.45 * RATE)):
        t = i / RATE
        freq = 45 + 75 * math.exp(-t * 28)
        phase = TAU * (45 * t + 75 * (1 - math.exp(-t * 28)) / 28)
        out.append(math.sin(phase) * math.exp(-t * 7) * 0.9 * (1 if freq else 0))
    return out


def snare(rng: random.Random):
    out = []
    y = 0.0
    for i in range(int(0.25 * RATE)):
        t = i / RATE
        noise = rng.uniform(-1, 1)
        y += 0.35 * (noise - y)  # soften the noise
        tone = math.sin(TAU * 185 * t) * math.exp(-t * 30)
        out.append((0.55 * (noise - y) + 0.4 * tone) * math.exp(-t * 18) * 0.5)
    return out


def hat(rng: random.Random, open_hat: bool = False):
    decay = 18 if open_hat else 60
    out = []
    previous = 0.0
    for i in range(int((0.3 if open_hat else 0.08) * RATE)):
        t = i / RATE
        noise = rng.uniform(-1, 1)
        high = noise - previous  # crude high-pass
        previous = noise
        out.append(high * math.exp(-t * decay) * 0.12)
    return out


def vinyl(track: Track, amount: float):
    rng = track.random
    for i in range(0, track.length, 1):
        if rng.random() < 0.00025:
            click = rng.uniform(-1, 1) * amount
            track.add(i, [click, click * 0.4, click * 0.1], pan=rng.uniform(-0.5, 0.5))
    hiss_gain = amount * 0.02
    for i in range(track.length):
        n = rng.uniform(-1, 1) * hiss_gain
        track.left[i] += n
        track.right[i] += n


def chord_notes(names):
    return [note(n) for n in names]


def drums(track: Track, bars: int, swing: float, pattern: str):
    rng = track.random
    k, s, h = kick(), snare(rng), None
    for bar in range(bars):
        for step in range(8):  # eighth notes
            beat = bar * 4 + step * 0.5
            if step % 2 == 1:
                beat += (swing - 0.5)  # swing the off-beats
            position = track.at(beat)
            if pattern == "lofi":
                if step in (0, 5) or (step == 3 and bar % 2 == 1):
                    track.add(position, k, gain=0.9)
                if step in (2, 6):
                    track.add(position, s, gain=0.8, pan=0.05)
            else:  # "groove"
                if step in (0, 3, 4):
                    track.add(position, k, gain=0.85)
                if step in (2, 6):
                    track.add(position, s, gain=0.75)
            h = hat(rng, open_hat=(step == 7 and bar % 4 == 3))
            track.add(position, h, pan=0.3, gain=0.8 if step % 2 == 0 else 0.55)


def late_night_commit() -> Track:
    """Lo-fi hip-hop, 78 BPM."""
    bars = 16
    track = Track(bpm=78, bars=bars, seed=1)
    progression = [
        ["F3", "A3", "C4", "E4"],
        ["E3", "G3", "B3", "D4"],
        ["D3", "F3", "A3", "C4"],
        ["C3", "E3", "G3", "B3"],
    ]
    roots = ["F2", "E2", "D2", "C2"]
    for bar in range(bars):
        chord = chord_notes(progression[bar % 4])
        for beat_offset, velocity in ((0, 1.0), (2.5, 0.7)):
            start = track.at(bar * 4 + beat_offset)
            for i, n in enumerate(chord):
                strum = int(i * 0.012 * RATE)
                track.add(start + strum, electric_piano(midi_hz(n), track.beat * 1.8, velocity),
                          pan=-0.3 + i * 0.2)
        root = midi_hz(note(roots[bar % 4]))
        track.add(track.at(bar * 4), bass(root, track.beat * 2.4))
        track.add(track.at(bar * 4 + 3), bass(root * 1.5, track.beat * 0.8), gain=0.8)
    drums(track, bars, swing=0.58, pattern="lofi")
    vinyl(track, 0.25)
    track.lowpass(5200)
    track.normalize()
    return track


def refactor_groove() -> Track:
    """Upbeat, arpeggiated, 96 BPM."""
    bars = 16
    track = Track(bpm=96, bars=bars, seed=2)
    progression = [
        ["A3", "C4", "E4", "G4"],
        ["F3", "A3", "C4", "E4"],
        ["C4", "E4", "G4", "B4"],
        ["G3", "B3", "D4", "F#4"],
    ]
    roots = ["A1", "F1", "C2", "G1"]
    arpeggio = [0, 1, 2, 3, 2, 1, 2, 3]
    for bar in range(bars):
        chord = chord_notes(progression[bar % 4])
        for step in range(16):
            n = chord[arpeggio[step % 8]] + (12 if step >= 8 and bar % 2 else 0)
            pan = -0.4 if step % 2 else 0.4
            track.add(track.at(bar * 4 + step * 0.25), pluck(midi_hz(n), 0.5), pan=pan, gain=0.8)
        for i, n in enumerate(chord[:3]):
            track.add(track.at(bar * 4), pad(midi_hz(n - 12), track.beat * 3.5), pan=-0.2 + i * 0.2,
                      gain=0.6)
        root = midi_hz(note(roots[bar % 4]))
        for beat in (0, 1.5, 2, 3.5):
            track.add(track.at(bar * 4 + beat), bass(root * 2, track.beat * 0.45))
    drums(track, bars, swing=0.52, pattern="groove")
    track.lowpass(7500)
    track.normalize()
    return track


def deep_focus() -> Track:
    """Slow ambient pads without drums, 60 BPM."""
    bars = 16
    track = Track(bpm=60, bars=bars, seed=3)
    progression = [
        ["D3", "F#3", "A3", "C#4", "E4"],
        ["B2", "D3", "F#3", "A3", "C#4"],
        ["G2", "B2", "D3", "F#3", "A3"],
        ["A2", "C#3", "E3", "F#3", "B3"],
    ]
    for bar in range(0, bars, 2):
        chord = chord_notes(progression[(bar // 2) % 4])
        start = track.at(bar * 4)
        for i, n in enumerate(chord):
            track.add(start, pad(midi_hz(n), track.beat * 7.5), pan=-0.5 + i * 0.25)
        track.add(start, bass(midi_hz(chord[0] - 12), track.beat * 7.5), gain=0.5)
        # A few gentle melody notes.
        rng = track.random
        for beat in (1, 3.5, 5, 6.5):
            n = rng.choice(chord) + 12
            track.add(track.at(bar * 4 + beat), electric_piano(midi_hz(n), 1.5, 0.5), pan=0.3,
                      gain=0.7)
    track.lowpass(3800)
    track.normalize(0.7)
    return track


TRACKS = [
    ("coding-01", late_night_commit),
    ("coding-02", refactor_groove),
    ("ambient-01", deep_focus),
]


def main():
    out_dir = sys.argv[1] if len(sys.argv) > 1 else os.path.join("assets", "music")
    os.makedirs(out_dir, exist_ok=True)
    converter = shutil.which("afconvert")
    for name, build in TRACKS:
        print(f"rendering {name}...", flush=True)
        track = build()
        wav = os.path.join(out_dir, f"{name}.wav")
        track.write_wav(wav)
        if converter:
            m4a = os.path.join(out_dir, f"{name}.m4a")
            subprocess.run(
                [converter, "-f", "m4af", "-d", "aac", "-b", "160000", "-q", "127", wav, m4a],
                check=True,
            )
            os.remove(wav)
            print(f"  wrote {m4a}")
        else:
            print(f"  wrote {wav} (convert it to AAC .m4a before committing)")


if __name__ == "__main__":
    main()
