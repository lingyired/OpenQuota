import type { EasingFunction, TransitionConfig } from 'svelte/transition';

interface PageTransitionOptions {
  direction: number;
  duration: number;
  easing: EasingFunction;
  zIndex?: number;
}

type AppScreen = 'dashboard' | 'customize' | 'settings' | `provider:${string}`;

export function shouldSlideBetweenScreens(from: AppScreen, to: AppScreen) {
  return from !== to;
}

export function horizontalPageTransition(
  _node: Element,
  { direction, duration, easing, zIndex }: PageTransitionOptions,
): TransitionConfig {
  const distance = _node.getBoundingClientRect().width * direction;
  const layer = zIndex === undefined ? '' : `z-index: ${zIndex}; `;
  return {
    duration,
    easing,
    css: (t) => `${layer}transform: translate3d(${(1 - t) * distance}px, 0, 0);`,
  };
}
