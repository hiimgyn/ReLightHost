import { gsap } from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';

gsap.registerPlugin(ScrollTrigger);

export function animate() {
  const mm = gsap.matchMedia();

  // Reduced motion: no animation, and global.css never hides anything (no `js` class).
  mm.add('(prefers-reduced-motion: no-preference)', () => {
    gsap.timeline({ defaults: { duration: 0.9, ease: 'power3.out' } }).fromTo(
      '[data-hero]',
      { autoAlpha: 0, y: 28, filter: 'blur(6px)' },
      { autoAlpha: 1, y: 0, filter: 'blur(0px)', stagger: 0.1 },
    );

    gsap.fromTo(
      '#hero-shot',
      { rotationX: 22, scale: 0.9, y: 40 },
      {
        rotationX: 0,
        scale: 1,
        y: 0,
        ease: 'none',
        scrollTrigger: { trigger: '#hero-shot', start: 'top bottom', end: 'top 25%', scrub: 1 },
      },
    );

    gsap.fromTo(
      '#chain-shot',
      { scale: 0.92, autoAlpha: 0.4 },
      {
        scale: 1,
        autoAlpha: 1,
        ease: 'none',
        scrollTrigger: { trigger: '#chain-shot', start: 'top 95%', end: 'top 45%', scrub: 1 },
      },
    );

    gsap.utils.toArray<HTMLElement>('[data-parallax]').forEach((el) => {
      gsap.to(el, {
        y: Number(el.dataset.parallax),
        ease: 'none',
        scrollTrigger: { trigger: '#showcase', start: 'top bottom', end: 'bottom top', scrub: 1 },
      });
    });

    ScrollTrigger.batch('[data-reveal]', {
      start: 'top 88%',
      once: true,
      onEnter: (batch) =>
        gsap.fromTo(
          batch,
          { autoAlpha: 0, y: 32 },
          { autoAlpha: 1, y: 0, duration: 0.8, ease: 'power3.out', stagger: 0.08, overwrite: true },
        ),
    });

    document.fonts.ready.then(() => ScrollTrigger.refresh());
  });
}
