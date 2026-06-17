export function StatusBar({ sampleRate }: { sampleRate: number | null }) {
  const ok = sampleRate != null && sampleRate > 0;
  return (
    <footer className="statusbar mono">
      <span className="st-engine">
        <span className={`st-dot ${ok ? "st-dot--ok" : "st-dot--off"}`} />
        {ok ? "Engine OK" : "No audio device"}
      </span>
      <span className="st-sep">·</span>
      <span>{ok ? `${(sampleRate / 1000).toFixed(1)} kHz` : "— kHz"}</span>
      <span className="st-sep">·</span>
      <span className="st-rt">RT-SAFE</span>
      <span className="st-right">
        <span>Windows · WASAPI</span>
        <span className="st-sep">·</span>
        <span>compas 0.1.0 — Phase 1</span>
      </span>
    </footer>
  );
}
