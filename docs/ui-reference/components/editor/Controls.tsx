import { useRef, useState, type ButtonHTMLAttributes, type ReactNode } from 'react';
import { Icon } from './EditorIcon';

type IconButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  icon: string;
  label: string;
  active?: boolean;
  small?: boolean;
};

export function IconButton({ icon, label, active, small, className = '', ...props }: IconButtonProps) {
  return (
    <button type="button" aria-label={label} aria-pressed={active} data-tooltip={label}
      className={`icon-button ${active ? 'is-active' : ''} ${small ? 'is-small' : ''} ${className}`} {...props}>
      <Icon name={icon} size={small ? 16 : 20} />
    </button>
  );
}

export function Panel({ name, pinned, onPin, onClose, className = '', children }: {
  name: string; pinned: boolean; onPin: () => void; onClose: () => void; className?: string; children: ReactNode;
}) {
  const [position, setPosition] = useState({ x: 0, y: 0 });
  const panel = useRef<HTMLElement>(null);
  const drag = useRef<{ x: number; y: number; px: number; py: number } | null>(null);
  return (
    <section ref={panel} className={`floating-panel ${className}`} aria-label={`${name} panel`} style={{ translate: `${position.x}px ${position.y}px` }}>
      <div className="panel-handle-row">
        <button className="panel-grip" aria-label={`Move ${name} panel`} title="Drag to move panel"
          onPointerDown={event => {
            drag.current = { x: event.clientX, y: event.clientY, px: position.x, py: position.y };
            event.currentTarget.setPointerCapture(event.pointerId);
          }}
          onPointerMove={event => {
            if (!drag.current || !panel.current) return;
            const rect = panel.current.getBoundingClientRect();
            const nextX = drag.current.px + event.clientX - drag.current.x;
            const nextY = drag.current.py + event.clientY - drag.current.y;
            const baseX = rect.left - position.x;
            const baseY = rect.top - position.y;
            setPosition({
              x: Math.max(4 - baseX, Math.min(window.innerWidth - 40 - baseX, nextX)),
              y: Math.max(4 - baseY, Math.min(window.innerHeight - 50 - baseY, nextY)),
            });
          }}
          onPointerUp={() => { drag.current = null; }}
          onPointerCancel={() => { drag.current = null; }}
          onKeyDown={event => {
            const directions: Record<string, number[]> = { ArrowLeft: [-10, 0], ArrowRight: [10, 0], ArrowUp: [0, -10], ArrowDown: [0, 10] };
            const d = directions[event.key];
            if (d) { event.preventDefault(); setPosition(p => ({ x: p.x + d[0], y: p.y + d[1] })); }
          }}><span /></button>
      </div>
      <header className="panel-header">
        <h2>{name === 'Curves' ? 'Curves Adjustment' : name}</h2>
        <div className="panel-header-actions">
          <IconButton icon="pin" label={`Pin ${name} panel`} active={pinned} small onClick={onPin} />
          <IconButton icon="close" label={`Close ${name} panel`} small onClick={onClose} />
        </div>
      </header>
      {children}
    </section>
  );
}

export function SelectField({ label, value, options, onChange }: {
  label: string; value: string; options: string[]; onChange: (value: string) => void;
}) {
  return <label className="select-field"><span className="sr-only">{label}</span>
    <select aria-label={label} value={value} onChange={event => onChange(event.target.value)}>
      {options.map(option => <option key={option}>{option}</option>)}
    </select><Icon name="down" size={12} />
  </label>;
}

export function NumberField({ label, value, min = 0, max = 100, step = 1, suffix = '', onChange }: {
  label: string; value: string; min?: number; max?: number; step?: number; suffix?: string; onChange: (value: string) => void;
}) {
  return <label className="number-field"><span className="sr-only">{label}</span>
    <input aria-label={label} type="number" min={min} max={max} step={step} value={value}
      onChange={event => onChange(event.target.value)}
      onBlur={event => onChange(String(Math.max(min, Math.min(max, Number(event.target.value) || min))))} />
    {suffix && <span>{suffix}</span>}
  </label>;
}
