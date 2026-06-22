type AudioStatus = {
  audio_online: boolean;
  audio_restarting: boolean;
  audio_restarts: number;
  audio_error: string | null;
};

export function StatusBar({ sampleRate, audioStatus }: { sampleRate: number | null; audioStatus: AudioStatus | null }) {
  const ok = sampleRate != null && sampleRate > 0 && (audioStatus?.audio_online ?? true);
  const restarting = audioStatus?.audio_restarting ?? false;
  const label = restarting ? "Audio restarting" : ok ? "Engine OK" : "No audio device";
  const title = audioStatus?.audio_error || (audioStatus?.audio_restarts ? `Recovered ${audioStatus.audio_restarts} time(s)` : undefined);
  return (
    <footer className="statusbar mono">
      <span className="st-engine" title={title}>
        <span className={`st-dot ${restarting ? "st-dot--warn" : ok ? "st-dot--ok" : "st-dot--off"}`} />
        {label}
      </span>
      <span className="st-sep">·</span>
      <span>{ok ? `${(sampleRate / 1000).toFixed(1)} kHz` : "— kHz"}</span>
      <span className="st-sep">·</span>
      <span className="st-rt">RT-SAFE</span>
      <span className="st-right">
        <span>compas — public beta</span>
      </span>
    </footer>
  );
}
