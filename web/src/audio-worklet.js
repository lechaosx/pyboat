import { SeaSoundDSP } from './audio-dsp.js';
class SeaProcessor extends AudioWorkletProcessor {
  constructor() {
    super(); this.dsp = new SeaSoundDSP(sampleRate);
    this.port.onmessage = ({ data }) => {
      if (data.resetEvents) this.dsp.resetEvents();
      if (data.thunder) for (const event of data.thunder) this.dsp.queueThunder(event);
      if (data.impacts) for (const event of data.impacts) this.dsp.queueImpact(event);
      if (data.state) this.dsp.setState(data.state, data.elapsedSeconds);
      if (data.layers) this.dsp.layers = data.layers;
    };
  }
  process(_inputs, outputs) {
    const output = outputs[0];
    if (output?.length === 2) this.dsp.render(output[0], output[1]);
    return true;
  }
}
registerProcessor('sailing-sea', SeaProcessor);
