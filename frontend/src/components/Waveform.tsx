import { useCallback, useEffect, useRef } from "react";

interface Props {
  peaks: number[];
  /** Play-head position as a fraction 0..1 of the track. */
  positionFrac: number;
  onSeek?: (frac: number) => void;
}

interface GL {
  gl: WebGLRenderingContext;
  program: WebGLProgram;
  posLoc: number;
  colorLoc: WebGLUniformLocation | null;
  waveBuf: WebGLBuffer | null;
  headBuf: WebGLBuffer | null;
  waveVerts: number;
}

const VERT = `attribute vec2 a; void main(){ gl_Position = vec4(a, 0.0, 1.0); }`;
const FRAG = `precision mediump float; uniform vec4 u; void main(){ gl_FragColor = u; }`;

/**
 * Waveform overview rendered on a WebGL canvas (not DOM), with a Canvas-2D fallback if
 * WebGL is unavailable. Peaks are uploaded once per load; only the play-head is cheap to
 * redraw at telemetry rate. The zoomable/scrolling waveform (P1 follow-up) will reuse
 * this WebGL path with a scrolling vertex window.
 */
export function Waveform({ peaks, positionFrac, onSeek }: Props) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const glRef = useRef<GL | null>(null);

  // (Re)build the GL program + waveform vertex buffer when peaks change.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const gl = canvas.getContext("webgl", { antialias: true, alpha: true });
    if (!gl) {
      glRef.current = null;
      return;
    }

    const compile = (type: number, src: string): WebGLShader | null => {
      const s = gl.createShader(type);
      if (!s) return null;
      gl.shaderSource(s, src);
      gl.compileShader(s);
      return s;
    };
    const vs = compile(gl.VERTEX_SHADER, VERT);
    const fs = compile(gl.FRAGMENT_SHADER, FRAG);
    const program = gl.createProgram();
    if (!vs || !fs || !program) return;
    gl.attachShader(program, vs);
    gl.attachShader(program, fs);
    gl.linkProgram(program);

    const n = peaks.length;
    const verts = new Float32Array(Math.max(n, 1) * 4); // 2 points (x,y) per peak
    for (let i = 0; i < n; i++) {
      const x = n > 1 ? (i / (n - 1)) * 2 - 1 : 0;
      const h = Math.min(peaks[i], 1) * 0.95;
      verts[i * 4] = x;
      verts[i * 4 + 1] = -h;
      verts[i * 4 + 2] = x;
      verts[i * 4 + 3] = h;
    }
    const waveBuf = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, waveBuf);
    gl.bufferData(gl.ARRAY_BUFFER, verts, gl.STATIC_DRAW);

    glRef.current = {
      gl,
      program,
      posLoc: gl.getAttribLocation(program, "a"),
      colorLoc: gl.getUniformLocation(program, "u"),
      waveBuf,
      headBuf: gl.createBuffer(),
      waveVerts: n * 2,
    };

    return () => {
      gl.deleteBuffer(waveBuf);
      gl.deleteProgram(program);
      glRef.current = null;
    };
  }, [peaks]);

  // Draw (waveform + play-head). Runs on load and every position update.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    // Keep the backing store in sync with CSS size (HiDPI-aware).
    const dpr = window.devicePixelRatio || 1;
    const w = Math.max(1, Math.floor(canvas.clientWidth * dpr));
    const h = Math.max(1, Math.floor(canvas.clientHeight * dpr));
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }

    const r = glRef.current;
    const headX = positionFrac * 2 - 1;

    if (r) {
      const { gl } = r;
      gl.viewport(0, 0, w, h);
      gl.clearColor(0.1, 0.1, 0.14, 1);
      gl.clear(gl.COLOR_BUFFER_BIT);
      gl.useProgram(r.program);
      gl.enableVertexAttribArray(r.posLoc);

      gl.bindBuffer(gl.ARRAY_BUFFER, r.waveBuf);
      gl.vertexAttribPointer(r.posLoc, 2, gl.FLOAT, false, 0, 0);
      gl.uniform4f(r.colorLoc, 0.49, 0.55, 0.7, 1);
      gl.drawArrays(gl.LINES, 0, r.waveVerts);

      const head = new Float32Array([headX, -1, headX, 1]);
      gl.bindBuffer(gl.ARRAY_BUFFER, r.headBuf);
      gl.bufferData(gl.ARRAY_BUFFER, head, gl.DYNAMIC_DRAW);
      gl.vertexAttribPointer(r.posLoc, 2, gl.FLOAT, false, 0, 0);
      gl.uniform4f(r.colorLoc, 0.85, 0.27, 0.63, 1);
      gl.drawArrays(gl.LINES, 0, 2);
      return;
    }

    // Canvas-2D fallback.
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.clearRect(0, 0, w, h);
    ctx.fillStyle = "#1a1a26";
    ctx.fillRect(0, 0, w, h);
    const mid = h / 2;
    ctx.strokeStyle = "#7d8cb3";
    ctx.beginPath();
    const n = peaks.length;
    for (let i = 0; i < n; i++) {
      const x = n > 1 ? (i / (n - 1)) * w : 0;
      const amp = Math.min(peaks[i], 1) * mid * 0.95;
      ctx.moveTo(x, mid - amp);
      ctx.lineTo(x, mid + amp);
    }
    ctx.stroke();
    const px = positionFrac * w;
    ctx.strokeStyle = "#d946a0";
    ctx.beginPath();
    ctx.moveTo(px, 0);
    ctx.lineTo(px, h);
    ctx.stroke();
  }, [peaks, positionFrac]);

  const handleClick = useCallback(
    (e: React.PointerEvent<HTMLCanvasElement>) => {
      if (!onSeek) return;
      const rect = e.currentTarget.getBoundingClientRect();
      const frac = Math.min(Math.max((e.clientX - rect.left) / rect.width, 0), 1);
      onSeek(frac);
    },
    [onSeek],
  );

  return <canvas ref={canvasRef} className="waveform" onPointerDown={handleClick} />;
}
