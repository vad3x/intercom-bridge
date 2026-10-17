import type { Component, ComponentProps } from 'solid-js';
import { splitProps } from 'solid-js';

export interface BadgeProps extends ComponentProps<'span'> {
  variant?: 'success' | 'warning' | 'error' | 'info' | 'ghost' | 'neutral';
  size?: 'xs' | 'sm' | 'md';
}

export const Badge: Component<BadgeProps> = (props) => {
  const [local, rest] = splitProps(props, ['variant', 'size', 'class', 'children']);

  const variantClasses = () => {
    switch (local.variant) {
      case 'success':
        return 'bg-success/15 text-success border-success/30';
      case 'warning':
        return 'bg-warning/15 text-warning border-warning/40';
      case 'error':
        return 'bg-error/15 text-error border-error/30';
      case 'info':
        return 'bg-info/15 text-info border-info/30';
      case 'ghost':
        return 'bg-base-content/5 text-base-content/70 border-base-content/15';
      default:
        return 'bg-base-content/10 text-base-content/80 border-base-content/20';
    }
  };

  const sizeClasses = () => {
    switch (local.size) {
      case 'xs':
        return 'px-2 py-0.5 text-[10px]';
      case 'md':
        return 'px-3 py-1 text-xs';
      default:
        return 'px-2.5 py-0.5 text-xs';
    }
  };

  const classes = () =>
    [
      'inline-flex items-center gap-1.5 font-semibold uppercase tracking-wider rounded-full border shadow-sm transition-colors',
      variantClasses(),
      sizeClasses(),
      local.class || '',
    ]
      .filter(Boolean)
      .join(' ');

  return (
    <span class={classes()} {...rest}>
      {local.children}
    </span>
  );
};
