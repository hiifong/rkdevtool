/** Short synthesized USB plug/unplug cues via Web Audio (no asset files). */

let audioCtx: AudioContext | null = null;

function getCtx(): AudioContext | null {
  try {
    if (!audioCtx) {
      const Ctx = window.AudioContext || (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
      if (!Ctx) return null;
      audioCtx = new Ctx();
    }
    if (audioCtx.state === "suspended") {
      void audioCtx.resume();
    }
    return audioCtx;
  } catch {
    return null;
  }
}

function tone(
  ctx: AudioContext,
  frequency: number,
  startAt: number,
  duration: number,
  gainPeak: number,
) {
  const osc = ctx.createOscillator();
  const gain = ctx.createGain();
  osc.type = "sine";
  osc.frequency.value = frequency;
  gain.gain.setValueAtTime(0.0001, startAt);
  gain.gain.exponentialRampToValueAtTime(gainPeak, startAt + 0.012);
  gain.gain.exponentialRampToValueAtTime(0.0001, startAt + duration);
  osc.connect(gain);
  gain.connect(ctx.destination);
  osc.start(startAt);
  osc.stop(startAt + duration + 0.02);
}

/** Ascending two-tone: device inserted / appeared. */
export function playDeviceConnected() {
  const ctx = getCtx();
  if (!ctx) return;
  const t0 = ctx.currentTime;
  tone(ctx, 880, t0, 0.08, 0.08);
  tone(ctx, 1174.7, t0 + 0.07, 0.1, 0.07);
}

/** Descending two-tone: device removed. */
export function playDeviceDisconnected() {
  const ctx = getCtx();
  if (!ctx) return;
  const t0 = ctx.currentTime;
  tone(ctx, 740, t0, 0.08, 0.07);
  tone(ctx, 554.4, t0 + 0.07, 0.11, 0.06);
}
