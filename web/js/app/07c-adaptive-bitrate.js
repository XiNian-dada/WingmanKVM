import { shared } from './state.js';
import { getSetting, setSetting } from './00-settings.js';
import { request } from './03-session.js';
import { setDirectBitrate } from './07e-direct-h264.js';
import {
  connectionQuality, qualityBitrateText, renderQualityOverlay, syncQualityOverlay,
} from './07b-quality.js';

// Adaptive H.264 bitrate uses fresh WebRTC or Direct measurements. Each
// viewer requests a target; the shared encoder takes the lowest request.
// Native x264 tunes the next frame; the FFmpeg fallback needs slower changes.
const ADAPTIVE_POLICY = {
  // A one-second sample is "bad" when any congestion signal crosses its line:
  badLossPct: 5, //    ≥5 % of video packets lost: visible smearing/freezes
  badJitterMs: 200, // the jitter buffer holds >200 ms: delivery comes in bursts
  // RTT trend: well above the RTT seen while healthy also counts as bad,
  // which catches bufferbloat on links whose base RTT is already high.
  rttRiseFactor: 2,
  rttRiseMinMs: 100,
  severeLossPct: 15, // heavy loss: halve instead of stepping down
  // "Good" requires every known signal to be comfortably healthy. The gap
  // between the good and bad lines is the hysteresis band: samples inside it
  // reset both streaks and never change the bitrate.
  goodLossPct: 1,
  goodJitterMs: 80,
  badSamples: 3, //   ≈3 s of consecutive bad samples before lowering
  goodSamples: 30, // ≈30 s of consecutive good samples before raising
  decreaseFactor: 0.7,
  severeFactor: 0.5,
  increaseFactor: 1.25,
  // Under loss the target also never exceeds what actually arrived (+10 %).
  receivedHeadroom: 1.1,
  cooldownMs: 30000, // min spacing of decreases; the shared encoder enforces it too
  increaseIntervalMs: 60000, // recovery must not restart FFmpeg every 10 s
  settleMs: 5000, // ignore restart/decoder warm-up samples after a change
  raiseAfterBadMs: 60000, // no raise within 60 s of the last bad sample
  // Probing at capacity: a raise followed by a decrease within
  // probeFailWindowMs is a failed probe. Each consecutive failure doubles the
  // wait after the last bad sample (raiseAfterBadMs × 2^failures, at most
  // maxRaiseAfterBadMs), so a link that sits between two steps restarts the
  // encoder much less often. probeResetMs without any bad
  // sample clears the count; it must exceed maxRaiseAfterBadMs, otherwise the
  // count would clear before the longest wait has passed.
  // Queues can take tens of seconds to build after a probe. A 20 s window
  // missed real failures observed ~40 s after a raise.
  probeFailWindowMs: 90000,
  maxRaiseAfterBadMs: 240000,
  probeResetMs: 600000,
  stepKbps: 50,
  snapToMaxRatio: 0.95, //  a raise this close to the maximum goes all the way
};
function policyForEncoder(live) {
  return live ? { ...ADAPTIVE_POLICY, cooldownMs: 10000, increaseIntervalMs: 10000,
    goodSamples: 10, settleMs: 0, raiseAfterBadMs: 30000,
    increaseFactor: 1.05, stepKbps: 1, snapToMaxRatio: 1 } : ADAPTIVE_POLICY;
}
function newAdaptiveState(maxKbps, minKbps = 256) {
  const max = Math.max(1, Math.round(Number(maxKbps) || 4000));
  return {
    currentKbps: max,
    maxKbps: max,
    minKbps: Math.min(max, Math.max(1, Math.round(Number(minKbps) || 256))),
    badStreak: 0,
    goodStreak: 0,
    lastChangeAt: null,
    lastBadAt: null,
    rttBaselineMs: null,
    lastRaiseAt: null,
    probeFailures: 0,
  };
}
const known = (value) => typeof value === 'number' && Number.isFinite(value);
// Pure: returns { state, targetKbps, verdict } and never mutates its inputs.
// targetKbps is null when the bitrate should stay; otherwise the returned
// state already assumes the change (the caller reverts on a failed request).
function decideAdaptiveBitrate(previous, sample, now, policy = ADAPTIVE_POLICY) {
  const state = { ...previous },
    loss = known(sample?.packetLossPct) ? sample.packetLossPct : null,
    rtt = known(sample?.rttMs) ? sample.rttMs : null,
    jitter = known(sample?.jitterBufferMs) ? sample.jitterBufferMs : null,
    received = known(sample?.receivedKbps) ? sample.receivedKbps : null;
  if (state.lastChangeAt !== null && now - state.lastChangeAt < policy.settleMs) {
    state.badStreak = 0; state.goodStreak = 0;
    return { state, targetKbps: null, verdict: 'settling' };
  }
  if (loss === null && rtt === null && jitter === null) {
    state.badStreak = 0; state.goodStreak = 0;
    return { state, targetKbps: null, verdict: 'unknown' };
  }
  // Distance adds stable RTT; only growth above the best observed path
  // latency is evidence of a queue. Fixed thresholds punished healthy VPNs.
  if (rtt !== null) state.rttBaselineMs = state.rttBaselineMs === null ? rtt : Math.min(state.rttBaselineMs, rtt);
  const base = state.rttBaselineMs,
    rttRising = rtt !== null && base !== null && rtt >= base * policy.rttRiseFactor && rtt - base >= policy.rttRiseMinMs,
    bad =
      (loss !== null && loss >= policy.badLossPct) ||
      (jitter !== null && jitter >= policy.badJitterMs) ||
      rttRising,
    good =
      !bad &&
      (!known(sample?.decodedFps) || sample.decodedFps > 0) &&
      (loss === null || loss < policy.goodLossPct) &&
      (rtt === null || rtt <= base + Math.max(50, base * 0.25)) &&
      (jitter === null || jitter < policy.goodJitterMs);
  const verdict = bad ? 'bad' : good ? 'good' : 'neutral';
  if (bad) {
    state.badStreak = Math.min(state.badStreak + 1, policy.badSamples);
    state.goodStreak = 0;
    state.lastBadAt = now;
  } else if (good) {
    state.goodStreak = Math.min(state.goodStreak + 1, policy.goodSamples);
    state.badStreak = 0;
  } else {
    state.badStreak = 0;
    state.goodStreak = 0;
  }
  // Failed probes stretch the post-congestion hold (see probeFailWindowMs).
  if (state.probeFailures > 0 && state.lastBadAt !== null && now - state.lastBadAt >= policy.probeResetMs)
    state.probeFailures = 0;
  const raiseHoldMs = Math.min(
    policy.raiseAfterBadMs * 2 ** (state.probeFailures || 0),
    Math.max(policy.raiseAfterBadMs, policy.maxRaiseAfterBadMs),
  );
  const cooled = state.lastChangeAt === null || now - state.lastChangeAt >= policy.cooldownMs,
    round = (kbps) =>
      Math.min(state.maxKbps, Math.max(state.minKbps, Math.round(kbps / policy.stepKbps) * policy.stepKbps));
  let target = null;
  if (bad && state.badStreak >= policy.badSamples && cooled && state.currentKbps > state.minKbps) {
    const severe = loss !== null && loss >= policy.severeLossPct;
    let next = state.currentKbps * (severe ? policy.severeFactor : policy.decreaseFactor);
    if (loss !== null && loss >= policy.badLossPct && received !== null && received > 0)
      next = Math.min(next, received * policy.receivedHeadroom);
    target = round(next);
    if (target >= state.currentKbps) target = Math.max(state.minKbps, state.currentKbps - policy.stepKbps);
  } else if (
    good &&
    state.goodStreak >= policy.goodSamples &&
    cooled &&
    (state.lastChangeAt === null || now - state.lastChangeAt >= policy.increaseIntervalMs) &&
    state.currentKbps < state.maxKbps &&
    (state.lastBadAt === null || now - state.lastBadAt >= raiseHoldMs)
  ) {
    target = round(state.currentKbps * policy.increaseFactor);
    if (target >= state.maxKbps * policy.snapToMaxRatio) target = state.maxKbps;
    if (target <= state.currentKbps) target = null;
  }
  if (target === null || target === state.currentKbps) return { state, targetKbps: null, verdict };
  if (target > state.currentKbps) {
    state.lastRaiseAt = now;
  } else {
    // Lowering soon after a raise means the probe found the capacity limit.
    if (state.lastRaiseAt !== null && now - state.lastRaiseAt <= policy.probeFailWindowMs)
      state.probeFailures = Math.min((state.probeFailures || 0) + 1, 16);
    state.lastRaiseAt = null;
  }
  state.currentKbps = target;
  state.lastChangeAt = now;
  state.badStreak = 0;
  state.goodStreak = 0;
  return { state, targetKbps: target, verdict };
}

const adaptive = {
  enabled: getSetting('adaptive_bitrate', true) !== false,
  sessionId: null,
  transport: null,
  state: null,
  pending: false,
  restoreTimer: 0,
};
function adaptiveBitrateWantsStats() {
  return adaptive.enabled && ['webrtc_h264', 'direct_h264'].includes(shared.activeVideoTransport);
}
// A new viewer initially requests the configured maximum bitrate.
function resetAdaptiveBitrate(sessionId = null, maxKbps = null, minKbps = null, transport = 'webrtc_h264') {
  clearTimeout(adaptive.restoreTimer);
  adaptive.sessionId = sessionId;
  adaptive.transport = transport;
  adaptive.pending = false;
  adaptive.state = sessionId
    ? newAdaptiveState(maxKbps ?? shared.bootstrap?.webrtc?.bitrate_kbps, minKbps ?? 256)
    : null;
}
async function sendAdaptiveBitrate(kbps, previousKbps) {
  const sessionId = adaptive.sessionId;
  adaptive.pending = true;
  try {
    const result = adaptive.transport === 'direct_h264' ? await setDirectBitrate(kbps) : await request('/api/webrtc/bitrate', {
      method: 'POST',
      body: JSON.stringify({ session_id: sessionId, kbps }),
    });
    if (sessionId !== adaptive.sessionId || !adaptive.state) return true;
    Object.assign(adaptive.state, {
      currentKbps: Number(result?.kbps) || kbps,
      maxKbps: Number(result?.max_kbps) || adaptive.state.maxKbps,
      minKbps: Number(result?.min_kbps) || adaptive.state.minKbps,
    });
    return true;
  } catch (error) {
    if (sessionId !== adaptive.sessionId || !adaptive.state) return false;
    // Not applied: undo the optimistic value but keep lastChangeAt, which
    // backs off for a full cooldown before the next attempt.
    if (adaptive.state.currentKbps === kbps) adaptive.state.currentKbps = previousKbps;
    // A raise that never reached the encoder is not a probe.
    if (kbps > previousKbps) adaptive.state.lastRaiseAt = null;
    if (error?.status === 404 || error?.status === 401) adaptive.sessionId = null;
    return false;
  } finally {
    if (sessionId === adaptive.sessionId) adaptive.pending = false;
    renderQualityOverlay();
  }
}
// Called by sampleQuality() only after a fresh video stats sample.
function adaptiveBitrateTick(now = performance.now()) {
  const liveSession = adaptive.transport === 'direct_h264' ? shared.directSessionId : shared.webrtcSessionId;
  if (!adaptive.enabled || !adaptive.state || adaptive.pending || adaptive.sessionId !== liveSession) return;
  const q = connectionQuality,
    previousKbps = adaptive.state.currentKbps,
    { state, targetKbps } = decideAdaptiveBitrate(
      adaptive.state,
      { packetLossPct: q.packetLossPct, rttMs: q.rttMs, jitterBufferMs: q.jitterBufferMs,
        receivedKbps: q.bitrateKbps, decodedFps: q.fps },
      now,
      policyForEncoder(shared.latestWebrtcStatus?.live_bitrate ?? shared.bootstrap?.webrtc?.live_bitrate),
    );
  adaptive.state = state;
  if (targetKbps !== null) sendAdaptiveBitrate(targetKbps, previousKbps);
}
// Turning adaptation off hands the session back its configured bitrate.
async function restoreConfiguredBitrate() {
  clearTimeout(adaptive.restoreTimer);
  const s = adaptive.state;
  if (adaptive.enabled || !s || !adaptive.sessionId || s.currentKbps >= s.maxKbps) return;
  if (adaptive.pending) {
    adaptive.restoreTimer = setTimeout(restoreConfiguredBitrate, 1000);
    return;
  }
  const previous = s.currentKbps;
  s.currentKbps = s.maxKbps;
  if (!(await sendAdaptiveBitrate(s.maxKbps, previous)) && adaptive.sessionId)
    adaptive.restoreTimer = setTimeout(restoreConfiguredBitrate, ADAPTIVE_POLICY.cooldownMs);
}
function adaptiveBitrateText() {
  if (!adaptive.enabled) return '已关闭';
  const s = adaptive.state;
  if (!s || !adaptive.sessionId) return '—';
  const text = `${qualityBitrateText(s.currentKbps)} / ${qualityBitrateText(s.maxKbps)}`;
  return s.currentKbps < s.maxKbps ? `↓ ${text}` : text;
}
function setAdaptiveBitrateEnabled(enabled) {
  adaptive.enabled = enabled;
  setSetting('adaptive_bitrate', enabled);
  if (!enabled) restoreConfiguredBitrate();
  else clearTimeout(adaptive.restoreTimer);
  syncQualityOverlay();
  renderQualityOverlay();
}

// DOM wiring; main.js calls this once, in the original fragment order.
export function init() {
  // The Playwright suite drives the pure decision function directly. The live
  // policy is only exposed when the harness set window.__wingmanTestHooks
  // before the page loaded, never on a normal page.
  if (window.__wingmanTestHooks === true)
    window.__wingmanTest = Object.assign(window.__wingmanTest || {}, {
      decideAdaptiveBitrate,
      newAdaptiveState,
      adaptivePolicy: ADAPTIVE_POLICY,
      policyForEncoder,
      adaptiveBitrate: () => (adaptive.state ? { ...adaptive.state, enabled: adaptive.enabled } : { enabled: adaptive.enabled }),
    });
}

export {
  adaptive, adaptiveBitrateText, adaptiveBitrateTick, adaptiveBitrateWantsStats, resetAdaptiveBitrate,
  setAdaptiveBitrateEnabled,
};
