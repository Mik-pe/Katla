#!/usr/bin/env python3
"""Regenerate audible tiny codec fixtures; requires ffmpeg with libmp3lame and its native Vorbis encoder."""
from pathlib import Path
import math
import struct
import subprocess
import wave

ROOT = Path(__file__).resolve().parents[1]
def main():
    directory = ROOT / 'odin/audio/testdata'; directory.mkdir(parents=True, exist_ok=True)
    source = directory / 'tone.wav'
    with wave.open(str(source), 'wb') as output:
        output.setnchannels(1); output.setsampwidth(2); output.setframerate(24000)
        output.writeframes(b''.join(struct.pack('<h', round(math.sin(2*math.pi*440*i/24000)*16383)) for i in range(4800)))
    for name, options in [('tone.ogg', ['-ac','2','-c:a','vorbis','-strict','-2']), ('tone.mp3',['-c:a','libmp3lame','-b:a','64k']), ('tone.flac',['-c:a','flac'])]:
        subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-y','-i',str(source),*options,str(directory/name)], check=True)
    print(directory)
if __name__ == '__main__': main()
