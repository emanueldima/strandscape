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

    const dragThreshold = 3;
    let pointer = null;
    let dragged = false;
    canvas.addEventListener('pointerdown', (event) => {
      if (event.button !== 0 || pointer) return;
      pointer = { id: event.pointerId, x: event.clientX, y: event.clientY };
      dragged = false;
      canvas.setPointerCapture(event.pointerId);
    });
    canvas.addEventListener('pointermove', (event) => {
      if (!pointer || event.pointerId !== pointer.id) return;
      if (!(event.buttons & 1)) {
        pointer = null;
        return;
      }
      const x = event.clientX - pointer.x;
      const y = event.clientY - pointer.y;
      // Keep small click movements from becoming drags.
      if (!dragged && Math.hypot(x, y) < dragThreshold) return;
      dragged = true;
      const rect = canvas.getBoundingClientRect();
      if (rect.width > 0 && rect.height > 0) {
        app.orbit(-x / rect.width, -y / rect.height);
        requestDraw();
      }
      pointer.x = event.clientX;
      pointer.y = event.clientY;
    });
    for (const type of ['pointerup', 'pointercancel', 'lostpointercapture']) {
      canvas.addEventListener(type, (event) => {
        if (event.pointerId === pointer?.id) pointer = null;
      });
    }
    canvas.addEventListener('click', (event) => {
      if (dragged) return;
      const rect = canvas.getBoundingClientRect();
      if (rect.width <= 0 || rect.height <= 0) return;
      app.focus((event.clientX - rect.left) / rect.width * 2 - 1,
        1 - (event.clientY - rect.top) / rect.height * 2, rect.width / rect.height);
      requestDraw();
    });
    canvas.addEventListener('wheel', (event) => {
      event.preventDefault();
      // Normalize line/page scrolling to CSS pixels before proportional zoom.
      const lineHeight = 16;
      const unit = event.deltaMode === WheelEvent.DOM_DELTA_LINE ? lineHeight
        : event.deltaMode === WheelEvent.DOM_DELTA_PAGE ? canvas.clientHeight : 1;
      app.zoom(event.deltaY * unit);
      requestDraw();
    }, { passive: false });

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
