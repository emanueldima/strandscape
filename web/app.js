import init, { WebApp } from './pkg/strandscape.js';

async function run() {
  const canvas = document.getElementById('scene');
  const status = document.getElementById('status');
  const defaultFont = 'https://raw.githubusercontent.com/google/fonts/main/ofl/abeezee/ABeeZee-Regular.ttf';
  const fontUrl = new URLSearchParams(location.search).get('font') ?? defaultFont;

  function report(error) {
    status.textContent = `Strandscape: ${error.message ?? error}`;
    status.hidden = false;
    console.error(error);
  }

  try {
    const [response] = await Promise.all([fetch(fontUrl), init()]);
    if (!response.ok) throw new Error(`Font download returned HTTP ${response.status}`);
    const bytes = new Uint8Array(await response.arrayBuffer());
    let viewport = measure(canvas);
    canvas.width = viewport[0];
    canvas.height = viewport[1];
    const app = await WebApp.create(canvas, bytes, ...viewport);
    app.draw();
    status.hidden = true;

    let frame = null;
    function draw() {
      frame = null;
      try {
        const next = measure(canvas);
        if (next.some((value, index) => value !== viewport[index])) {
          canvas.width = next[0];
          canvas.height = next[1];
          app.resize(...next);
          viewport = next;
        }
        app.draw();
      } catch (error) {
        report(error);
      }
    }

    function requestDraw() {
      if (frame !== null) return;
      frame = requestAnimationFrame(draw);
    }

    // Rearm the query after each display-scale change.
    let scaleQuery;
    function watchScale() {
      scaleQuery = matchMedia(`(resolution: ${devicePixelRatio}dppx)`);
      scaleQuery.addEventListener('change', () => {
        requestDraw();
        watchScale();
      }, { once: true });
    }

    const observer = new ResizeObserver(requestDraw);
    observer.observe(canvas);
    window.addEventListener('resize', requestDraw);
    watchScale();
  } catch (error) {
    report(error);
  }
}

function measure(canvas) {
  const rect = canvas.getBoundingClientRect();
  const scale = devicePixelRatio;
  return [Math.round(rect.width * scale), Math.round(rect.height * scale), scale];
}

run();
