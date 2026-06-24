/** Vertical VU meter. `level` is linear 0..~1; rendered with the standard
 *  green→amber→red ramp, or a desaturated cyan ramp for streaming (remote) decks.
 *  Fills its container's height by default. */
export function Meter({
  level,
  height,
  streaming = false,
}: {
  level: number;
  height?: number;
  streaming?: boolean;
}) {
  const lit = Math.min(1, Math.sqrt(Math.max(0, level)));
  const ramp = streaming
    ? "linear-gradient(to top,#246b7a 0 70%,#1d8fa8 70% 100%)"
    : "linear-gradient(to top,#3ddc97 0 60%,#ffb020 60% 85%,#ff3b5c 85% 100%)";
  return (
    <div className="meter" style={{ height: height ?? "100%" }}>
      <div className="meter-fill" style={{ background: ramp }} />
      <div className="meter-mask" style={{ height: `${(1 - lit) * 100}%` }} />
    </div>
  );
}
