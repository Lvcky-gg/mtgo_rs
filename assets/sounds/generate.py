"""Generate the app's original, short mono PCM sound cues. No external samples."""
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
    low = 0.0
    for i in range(round(duration * RATE)):
        t = i / RATE
        white = rng.uniform(-1, 1)
        low += 0.12 * (white - low)
        # Smooth both edges to keep even short cues free of clicks.
        edge = min(1.0, t / 0.006, (duration - t) / 0.025)
        samples.append(max(-1, min(1, voice(t, low, white - low) * edge)))
    with wave.open(str(ROOT / f'{name}.wav'), 'wb') as output:
        output.setnchannels(1)
        output.setsampwidth(2)
        output.setframerate(RATE)
        output.writeframes(b''.join(struct.pack('<h', round(v * 32767)) for v in samples))

def tone(t, frequency):
    return math.sin(2 * math.pi * frequency * t)

render('card-draw', .15, lambda t, low, high:
       .16 * high * math.sin(math.pi * t / .15) ** 2 + .08 * low)
render('card-play', .20, lambda t, low, high:
       (.20 * tone(t, 165) + .15 * low + .035 * high) * math.exp(-26 * t))
render('spell-cast', .38, lambda t, low, high:
       (.10 * math.sin(2 * math.pi * (440 * t + 420 * t * t))
        + .055 * math.sin(2 * math.pi * (660 * t + 630 * t * t))
        + .04 * high) * math.sin(math.pi * t / .38) ** 2)
render('spell-resolve', .40, lambda t, low, high:
       (.12 * tone(t, 523.25) + .065 * tone(t, 659.25)
        + .035 * tone(t, 783.99)) * math.exp(-10 * t))
render('damage', .23, lambda t, low, high:
       (.24 * math.sin(2 * math.pi * (115 * t - 90 * t * t))
        + .16 * low + .02 * high) * math.exp(-18 * t))
render('your-turn', .65, lambda t, low, high:
       .11 * tone(t, 659.25) * math.exp(-11 * t)
       + (0 if t < .16 else .14 * tone(t - .16, 880) * math.exp(-10 * (t - .16))))
