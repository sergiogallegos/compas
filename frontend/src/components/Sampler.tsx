import type { SamplerController } from "../hooks/useSampler";
import { Knob } from "./Knob";

/**
 * Sampler / performance pads panel. Click an empty pad to load a file; press a loaded pad to
 * fire it (one-shot, overlapping) — or toggle ⟳ for a looped pad that starts/stops on press.
 */
export function Sampler({ sampler, onClose }: { sampler: SamplerController; onClose: () => void }) {
  return (
    <div className="sampler">
      <div className="sampler-bar">
        <span className="overline">SAMPLER</span>
        <Knob value={sampler.gain} min={0} max={1.5} size={30} label="LEVEL" onChange={sampler.setGain} />
        <span className="mono sampler-hint">click empty → load · press → fire · ⟳ loop · ✕ clear</span>
        <button className="chip sampler-close" onClick={onClose} title="Close">✕</button>
      </div>
      <div className="spad-grid">
        {sampler.pads.map((pad, i) => (
          <div
            key={i}
            className={`spad ${pad.name ? "spad--loaded" : ""} ${pad.loop ? "spad--loop" : ""}`}
            onPointerDown={() => (pad.name ? sampler.trigger(i) : sampler.load(i))}
            title={pad.name ?? "Load a sample"}
          >
            <span className="spad-num">{i + 1}</span>
            <span className="spad-name">{pad.loading ? "…" : (pad.name ?? "EMPTY")}</span>
            {pad.name && (
              <div className="spad-foot" onPointerDown={(e) => e.stopPropagation()}>
                <button
                  className={`spad-mini ${pad.loop ? "spad-mini--on" : ""}`}
                  onClick={() => sampler.toggleLoop(i)}
                  title="Loop mode"
                >
                  ⟳
                </button>
                <button className="spad-mini" onClick={() => sampler.clear(i)} title="Clear pad">
                  ✕
                </button>
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
