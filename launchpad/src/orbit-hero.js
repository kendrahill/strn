const hero = document.querySelector('.orbit-hero');
const scene = hero.querySelector('.planet-scene');
const concept = new URLSearchParams(location.search).get('concept');
if (['aurora', 'stardust'].includes(concept)) scene.dataset.concept = concept;

function animateStars(canvas, context) {
  let width, height, stars, previousTime, frame;
  const pointer = { x: 0, y: 0, active: false };
  let visible = true;

  function resize() {
    width = hero.clientWidth;
    height = hero.clientHeight;
    const scale = Math.min(window.devicePixelRatio || 1, 2);
    canvas.width = Math.round(width * scale);
    canvas.height = Math.round(height * scale);
    context.setTransform(scale, 0, 0, scale, 0, 0);
    const count = Math.min(460, Math.ceil(width * height / 2900));
    stars = Array.from({ length: count }, () => {
      const depth = Math.random();
      return {
        x: Math.random() * width, y: Math.random() * height,
        dx: 0, dy: 0, speed: 2.2 + depth * 5.4,
        radius: .45 + depth * .9, opacity: .23 + depth * .55,
      };
    });
  }

  function draw(time) {
    const dt = Math.min((time - (previousTime ?? time)) / 1000, .05);
    previousTime = time;
    context.clearRect(0, 0, width, height);
    context.fillStyle = '#e1efed';
    const reach = Math.min(210, width * .4);
    const ease = 1 - Math.exp(-dt * 3.8);
    for (const star of stars) {
      star.x = (star.x + star.speed * dt) % width;
      star.y = (star.y - star.speed * .38 * dt + height) % height;
      const vx = pointer.x - star.x;
      const vy = pointer.y - star.y;
      const distance = Math.hypot(vx, vy);
      const pull = pointer.active ? .88 * Math.max(0, 1 - distance / reach) ** 2 : 0;
      star.dx += (vx * pull - star.dx) * ease;
      star.dy += (vy * pull - star.dy) * ease;
      context.globalAlpha = star.opacity;
      context.beginPath();
      context.arc(star.x + star.dx, star.y + star.dy, star.radius, 0, Math.PI * 2);
      context.fill();
    }
    frame = requestAnimationFrame(draw);
  }

  hero.addEventListener('pointermove', event => {
    if (event.pointerType === 'touch') return;
    const rect = hero.getBoundingClientRect();
    pointer.x = event.clientX - rect.left;
    pointer.y = event.clientY - rect.top;
    pointer.active = true;
  });
  function release() { pointer.active = false; }
  hero.addEventListener('pointerleave', release);
  hero.addEventListener('pointercancel', release);
  window.addEventListener('blur', release);

  function sync() {
    cancelAnimationFrame(frame);
    previousTime = undefined;
    if (visible && !document.hidden) frame = requestAnimationFrame(draw);
  }
  resize();
  new ResizeObserver(resize).observe(hero);
  new IntersectionObserver(([entry]) => {
    visible = entry.isIntersecting;
    sync();
  }).observe(hero);
  document.addEventListener('visibilitychange', sync);
  sync();
}
const canvas = hero.querySelector('canvas');
const context = canvas.getContext('2d');
if (context) animateStars(canvas, context);
else canvas.style.backgroundImage = 'url(' + new URL('./stars-far.svg', import.meta.url) + ')';
