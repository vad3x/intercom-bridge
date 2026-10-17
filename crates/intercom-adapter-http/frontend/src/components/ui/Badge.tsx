import type { JSX, ParentComponent } from 'solid-js';

export interface BadgeProps {
  variant?:
    | 'primary'
    | 'secondary'
    | 'accent'
    | 'success'
    | 'warning'
    | 'error'
    | 'info'
    | 'neutral';
  size?: 'xs' | 'sm' | 'md' | 'lg';
  outline?: boolean;
  class?: string;
  children?: JSX.Element;
}

export const Badge: ParentComponent<BadgeProps> = (props) => {
  const variantClass = () => {
    switch (props.variant) {
      case 'primary':
        return 'badge-primary';
      case 'secondary':
        return 'badge-secondary';
      case 'accent':
        return 'badge-accent';
      case 'success':
        return 'badge-success';
      case 'warning':
        return 'badge-warning';
      case 'error':
        return 'badge-error';
      case 'info':
        return 'badge-info';
      default:
        return 'badge-neutral';
    }
  };

  const sizeClass = () => {
    switch (props.size) {
      case 'xs':
        return 'badge-xs';
      case 'sm':
        return 'badge-sm';
      case 'lg':
        return 'badge-lg';
      default:
        return 'badge-sm';
    }
  };

  return (
    <span
      class={`badge ${variantClass()} ${sizeClass()} ${props.outline ? 'badge-outline' : ''} ${props.class || ''}`}
    >
      {props.children}
    </span>
  );
};
