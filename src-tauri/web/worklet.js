class RezoPlayer extends AudioWorkletProcessor {
  constructor(options) {
    super();
    const o = (options && options.processorOptions) || {};
    this.srcRate = o.srcRate || 48000;
    this.step = this.srcRate / sampleRate;
    this.cap = 1 << 16;
    this.mask = this.cap - 1;
    this.L = new Float32Array(this.cap);
    this.R = new Float32Array(this.cap);
    this.w = 0;
    this.rp = 0;
    this.minTarget = 0.03 * this.srcRate;
    this.target = this.minTarget;
    this.maxTarget = 0.16 * this.srcRate;
    this.started = false;
    this.underruns = 0;
    this.since = 0;
    this.port.onmessage = (e) => this.push(new Int16Array(e.data));
  }

  push(s16) {
    const n = s16.length >> 1;
    for (let i = 0; i < n; i++) {
      const k = (this.w + i) & this.mask;
      this.L[k] = s16[2 * i] / 32768;
      this.R[k] = s16[2 * i + 1] / 32768;
    }
    this.w += n;
  }

  process(_inputs, outputs) {
    const out = outputs[0];
    const oL = out[0];
    const oR = out[1] || out[0];
    const n = oL.length;
    let fill = this.w - this.rp;

    if (!this.started) {
      if (fill >= this.target) this.started = true;
      else return true;
    }
    if (fill < n * this.step + 2) {
      this.started = false;
      this.underruns++;
      this.target = Math.min(this.target * 1.4, this.maxTarget);
      return true;
    }
    if (fill > this.target + 0.08 * this.srcRate) {
      this.rp = this.w - this.target;
      fill = this.target;
    }
    const err = (fill - this.target) / this.target;
    const step = this.step * (1 + Math.max(-0.003, Math.min(0.003, err * 0.002)));

    let rp = this.rp;
    for (let i = 0; i < n; i++) {
      const i0 = Math.floor(rp);
      const f = rp - i0;
      const a = i0 & this.mask;
      const b = (i0 + 1) & this.mask;
      oL[i] = this.L[a] + (this.L[b] - this.L[a]) * f;
      oR[i] = this.R[a] + (this.R[b] - this.R[a]) * f;
      rp += step;
    }
    this.rp = rp;

    this.since += n;
    if (this.since > sampleRate * 2) {
      this.since = 0;
      if (this.target > this.minTarget) this.target = Math.max(this.minTarget, this.target * 0.9);
      this.port.postMessage({ fillMs: ((this.w - this.rp) / this.srcRate) * 1000, underruns: this.underruns });
    }
    return true;
  }
}
registerProcessor('rezo-player', RezoPlayer);
