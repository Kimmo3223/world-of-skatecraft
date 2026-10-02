#!/usr/bin/env python3
"""Converts the skate sounds out of your own Skate 3 install (`game/data/audio`) into the WAVs the
WoW skate mode plays: seamless loops for rolling, grinding and wind, and one-shot sets for pops,
landings, bails and skids.

    nix-shell -p vgmstream "python3.withPackages(p:[p.numpy])" \
        --run 'python3 tools/skate_audio.py "/path/to/Skate 3/data" skate-audio'

Samples are picked by bank and index; the Skate 3 banks carry no names.
"""
import os, struct, subprocess, sys, tempfile, wave

import numpy as np

sys.path.insert(0, os.path.dirname(__file__))
from eb_extract import unpack  # noqa: E402

# Loops: (output, source, start s, cut from the end s). Grain recordings end on a transient.
LOOPS = [
    ('roll_concrete', ('grain', 'concrete_smooth_hard'), 1.0, 2.0),
    ('roll_asphalt', ('grain', 'asphalt_rough_soft'), 1.0, 2.0),
    ('grind_trucks', ('GRINDS', 8), 1.5, 1.0),
    ('grind_board', ('GRINDS', 5), 1.0, 1.0),
    ('wind', ('sense_of_speed', 1), 1.5, 1.0),
]
# One-shot sets: (prefix, bank, indices).
SHOTS = [
    ('pop', 'Sk8_Air_Flip_Tricks', range(11, 15)),
    ('land', 'Sk8_Air_Flip_Tricks', range(15, 19)),
    ('land_hard', 'Sk8_Air_Flip_Tricks', range(19, 25)),
    ('flip', 'Sk8_Air_Flip_Tricks', range(25, 31)),
    ('bail', 'Bodyslide', [2, 3, 4, 5, 12, 14]),
    ('skid', 'WHEEL_SKID_BANK', range(80, 97)),
]
CROSSFADE_S = 0.3


def read_wav(path):
    w = wave.open(path)
    a = np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16).astype(np.float32)
    if w.getnchannels() > 1:
        a = a.reshape(-1, w.getnchannels()).mean(1)
    return a, w.getframerate()


def write_wav(path, a, rate):
    w = wave.open(path, 'wb')
    w.setnchannels(1)
    w.setsampwidth(2)
    w.setframerate(rate)
    w.writeframes(np.clip(a, -32768, 32767).astype(np.int16).tobytes())
    w.close()


def decode(src, out, subsong=None):
    cmd = ['vgmstream-cli', '-o', out]
    if subsong is not None:
        cmd += ['-s', str(subsong)]
    subprocess.run(cmd + [src], check=True, stdout=subprocess.DEVNULL)


def seamless(a, rate, start, cut):
    body = a[int(start * rate):len(a) - int(cut * rate)]
    x = int(CROSSFADE_S * rate)
    loop = body[:-x].copy()
    fade = np.linspace(0.0, 1.0, x, dtype=np.float32)
    loop[:x] = body[:x] * fade + body[-x:] * (1.0 - fade)
    return loop


def main(data, out):
    audio = os.path.join(data, 'audio')
    os.makedirs(out, exist_ok=True)
    tmp = tempfile.mkdtemp(prefix='skate-audio-')
    banks = os.path.join(tmp, 'banks')
    unpack(os.path.join(audio, 'audiofiles.big'), banks)
    unpack(os.path.join(audio, 'grains.big'), banks)

    def source(spec, name):
        bank, which = spec
        wav = os.path.join(tmp, name + '.wav')
        if bank == 'grain':
            d = open(os.path.join(banks, which + '.grain'), 'rb').read()
            snr = os.path.join(tmp, name + '.snr')
            open(snr, 'wb').write(d[struct.unpack('>I', d[:4])[0]:])
            decode(snr, wav)
        else:
            decode(os.path.join(banks, bank + '.abk'), wav, which)
        return read_wav(wav)

    for name, spec, start, cut in LOOPS:
        a, rate = source(spec, name)
        write_wav(os.path.join(out, name + '.wav'), seamless(a, rate, start, cut), rate)
        print(name)
    for prefix, bank, indices in SHOTS:
        for n, i in enumerate(indices, 1):
            a, rate = source((bank, i), f'{prefix}_{n}')
            write_wav(os.path.join(out, f'{prefix}_{n}.wav'), a, rate)
        print(f'{prefix} x{len(indices)}')


if __name__ == '__main__':
    main(os.path.expanduser(sys.argv[1]), sys.argv[2])
