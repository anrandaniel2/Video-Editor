import { useRef, useState, type CSSProperties } from 'react';
import { workspaces, type WorkspaceId, type WorkspaceState } from '../../data/workspaces';

export function DocumentCanvas({ workspace, state, patch }: {
  workspace: WorkspaceId; state: WorkspaceState; patch: (state: Partial<WorkspaceState>) => void;
}) {
  const drag = useRef<{ x: number; y: number; px: number; py: number } | null>(null);
  const [imageError, setImageError] = useState(false);
  const navigating = state.selectedTool === 'hand';
  return <div className={`document-viewport ${navigating ? 'can-pan' : ''} ${state.selectedTool === 'zoom' ? 'can-zoom' : ''}`}
    aria-label={workspaces[workspace].title}
    onPointerDown={event => {
      if (state.selectedTool === 'zoom') {
        patch({ zoom: Math.min(2.5, state.zoom + 0.15) });
      } else if (navigating) {
        drag.current = { x: event.clientX, y: event.clientY, px: state.pan.x, py: state.pan.y };
        event.currentTarget.setPointerCapture(event.pointerId);
      }
    }}
    onPointerMove={event => {
      if (!drag.current) return;
      patch({ pan: {
        x: Math.max(-400, Math.min(400, drag.current.px + event.clientX - drag.current.x)),
        y: Math.max(-300, Math.min(300, drag.current.py + event.clientY - drag.current.y)),
      } });
    }}
    onPointerUp={() => { drag.current = null; }}
    onPointerCancel={() => { drag.current = null; }}>
    <div className={`document-artwork artwork-${workspace}`} style={{ transform: `translate(${state.pan.x}px, ${state.pan.y}px) scale(${state.zoom})` } as CSSProperties}>
      {!imageError ? <img src={workspaces[workspace].artwork} alt={workspaces[workspace].title} draggable={false} onError={() => setImageError(true)} /> : <div className="artwork-fallback">Document preview unavailable</div>}
      {state.toggles.includes('grid') && <div className="document-grid" />}
    </div>
  </div>;
}
