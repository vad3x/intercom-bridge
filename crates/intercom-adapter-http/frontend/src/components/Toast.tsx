import type { Component } from 'solid-js';
import { Show } from 'solid-js';

export interface ToastProps {
  message: string | null;
  type?: 'info' | 'success' | 'error';
}

export const Toast: Component<ToastProps> = (props) => {
  const alertTypeClass = () => {
    switch (props.type) {
      case 'success':
        return 'alert-success';
      case 'error':
        return 'alert-error';
      default:
        return 'alert-info';
    }
  };

  return (
    <Show when={props.message}>
      <div class="toast toast-end toast-bottom z-50 p-4">
        <div class={`alert ${alertTypeClass()} shadow-lg text-sm font-semibold flex items-center`}>
          <span>{props.message}</span>
        </div>
      </div>
    </Show>
  );
};
