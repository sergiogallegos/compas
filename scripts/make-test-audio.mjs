// Synthesizes royalty-free test tracks (16-bit PCM WAV) for exercising the engine:
// a four-on-the-floor kick, snare on 2 & 4, off-beat hats, and a simple bassline at a
// known BPM. No dependencies. Run: node scripts/make-test-audio.mjs
import { writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const OUT = join(dirname(fileURLToPath(import.meta.url)), "..", "samples");
mkdirSync(OUT, { recursive: true });

const SR = 44100;

function writeWav(path, interleaved, channels = 2, sampleRate = SR) {
  const dataSize = interleaved.length * 2;
  const buf = Buffer.alloc(44 + dataSize);
  buf.write("RIFF", 0);
  buf.writeUInt32LE(36 + dataSize, 4);
  buf.write("WAVE", 8);
  buf.write("fmt ", 12);
  buf.writeUInt32LE(16, 16);
  buf.writeUInt16LE(1, 20); // PCM
  buf.writeUInt16LE(channels, 22);
  buf.writeUInt32LE(sampleRate, 24);
  buf.writeUInt32LE(sampleRate * channels * 2, 28);
  buf.writeUInt16LE(channels * 2, 32);
  buf.writeUInt16LE(16, 34);
  buf.write("data", 36);
  buf.writeUInt32LE(dataSize, 40);
  let o = 44;
  for (let i = 0; i < interleaved.length; i++) {
    const s = Math.max(-1, Math.min(1, interleaved[i]));
    buf.writeInt16LE((s * 32767) | 0, o);
    o += 2;
  }
  writeFileSync(path, buf);
}

// deterministic pseudo-noise (no Math.random needed)
function noise(n) {
  const x = Math.sin(n * 12.9898) * 43758.5453;
  return (x - Math.floor(x)) * 2 - 1;
}

function buildTrack(bpm, seconds, rootHz) {
  const total = Math.floor(seconds * SR);
  const out = new Float32Array(total * 2);
  const spb = (60 / bpm) * SR; // samples per beat
  const add = (i, l, r) => {
    if (i < 0 || i >= total) return;
    out[i * 2] += l;
    out[i * 2 + 1] += r;
  };

  const beats = Math.floor(total / spb);
  for (let b = 0; b < beats; b++) {
    const t0 = Math.floor(b * spb);

    // kick: pitch-dropping sine with fast decay (every beat)
    for (let n = 0; n < SR * 0.22; n++) {
      const env = Math.exp(-n / (SR * 0.08));
      const f = 45 + 90 * Math.exp(-n / (SR * 0.03));
      const s = Math.sin((2 * Math.PI * f * n) / SR) * env * 0.9;
      add(t0 + n, s, s);
    }

    // snare on beats 2 & 4 (0-indexed 1 & 3): noise burst
    if (b % 4 === 1 || b % 4 === 3) {
      for (let n = 0; n < SR * 0.16; n++) {
        const env = Math.exp(-n / (SR * 0.05));
        const s = noise(t0 + n) * env * 0.35;
        add(t0 + n, s, s);
      }
    }

    // bass note per beat following a 4-step root/fifth pattern
    const steps = [1, 1.5, 1, 1.335];
    const f = rootHz * steps[b % steps.length];
    for (let n = 0; n < spb * 0.9; n++) {
      const env = Math.exp(-n / (SR * 0.25)) * 0.3;
      const s = Math.sin((2 * Math.PI * f * n) / SR) * env;
      add(t0 + n, s, s);
    }

    // off-beat hats (half-beat): short bright noise
    const h = Math.floor(t0 + spb / 2);
    for (let n = 0; n < SR * 0.04; n++) {
      const env = Math.exp(-n / (SR * 0.012));
      const s = noise(h + n * 3) * env * 0.18;
      add(h + n, s, s);
    }
  }

  // gentle limiter
  for (let i = 0; i < out.length; i++) out[i] = Math.tanh(out[i] * 1.1);
  return out;
}

writeWav(join(OUT, "compas-test-120bpm.wav"), buildTrack(120, 32, 55)); // A1 root
writeWav(join(OUT, "compas-test-128bpm.wav"), buildTrack(128, 32, 65.4)); // C2 root
console.log(`wrote 2 test WAVs to ${OUT}`);
