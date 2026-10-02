"""Generate short, warm arena cues using filtered noise and damped resonances."""
from pathlib import Path
import math
import random
import struct
import wave

RATE = 44100
ROOT = Path(__file__).resolve().parent


def render(name, duration, voice):
    rng = random.Random(name)
    samples = []
    low = mid = output = 0.0
    for i in range(round(duration * RATE)):
        t = i / RATE
        white = rng.uniform(-1, 1)
        # Paper/felt texture, band-limited instead of raw high-frequency noise.
        low += 0.035 * (white - low)
        mid += 0.20 * (white - mid)
        paper = mid - low
        attack = min(1.0, t / 0.014)
        release = min(1.0, (duration - t) / 0.045)
        envelope = math.sin(attack * math.pi / 2) ** 2 * math.sin(release * math.pi / 2) ** 2
        signal = voice(t, low, paper) * envelope
        # Round remaining sharp transients before converting to PCM.
        output += 0.28 * (signal - output)
        samples.append(output)
    # Ensure the filter tail reaches zero smoothly at the file boundary.
    for i in range(min(256, len(samples))):
        samples[-1 - i] *= i / 256
    with wave.open(str(ROOT / f'{name}.wav'), 'wb') as output_file:
        output_file.setnchannels(1)
        output_file.setsampwidth(2)
        output_file.setframerate(RATE)
        output_file.writeframes(b''.join(struct.pack('<h', round(max(-1, min(1, v)) * 32767)) for v in samples))


def tone(t, frequency):
    return math.sin(2 * math.pi * frequency * t)


render('card-draw', .18, lambda t, low, paper:
       .42 * paper * math.sin(math.pi * t / .18) ** 2)
render('card-play', .16, lambda t, low, paper:
       (.11 * tone(t, 190) + .22 * low + .15 * paper) * math.exp(-32 * t))
render('spell-cast', .24, lambda t, low, paper:
       .40 * paper * math.sin(math.pi * t / .24) ** 2 + .05 * low)
render('spell-resolve', .20, lambda t, low, paper:
       (.09 * tone(t, 230) + .20 * low + .12 * paper) * math.exp(-24 * t))
render('damage', .18, lambda t, low, paper:
       (.12 * tone(t, 105) + .28 * low + .09 * paper) * math.exp(-26 * t))
render('your-turn', .34, lambda t, low, paper:
       (.10 * tone(t, 740) + .025 * tone(t, 1170)) * math.exp(-16 * t))
