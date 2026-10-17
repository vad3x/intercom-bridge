import type { JSX, ParentComponent } from 'solid-js';

export interface CardProps {
  bordered?: boolean;
  class?: string;
  children?: JSX.Element;
}

export const Card: ParentComponent<CardProps> = (props) => {
  return (
    <div
      class={`card bg-base-100 shadow-md ${props.bordered ? 'border border-base-content/10' : ''} ${props.class || ''}`}
    >
      {props.children}
    </div>
  );
};

export const CardBody: ParentComponent<{ class?: string; children?: JSX.Element }> = (props) => {
  return <div class={`card-body p-4 sm:p-6 ${props.class || ''}`}>{props.children}</div>;
};

export const CardTitle: ParentComponent<{ class?: string; children?: JSX.Element }> = (props) => {
  return (
    <h2
      class={`card-title text-base sm:text-lg font-bold flex items-center gap-2 ${props.class || ''}`}
    >
      {props.children}
    </h2>
  );
};
