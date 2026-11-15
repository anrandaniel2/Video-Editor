import { useCallback, useEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { Icon } from './components/editor/EditorIcon';
import { IconButton } from './components/editor/Controls';
import { DocumentCanvas } from './components/editor/DocumentCanvas';
import { TextControls, WorkspacePanels, type PanelActions } from './components/editor/WorkspacePanels';
import { initialState, studios, workspaces, type WorkspaceId, type WorkspaceState } from './data/workspaces';

type MenuItem = { label: string; icon: string; action: () => void; checked?: boolean; shortcut?: string };
type MenuState = { title: string; x: number; y: number; items: MenuItem[] };
const personaOrder: WorkspaceId[] = ['sketch', 'vector', 'pixel', 'layout'];

function Modal({ title, onClose, children }: { title: string; onClose: () => void; children: ReactNode }) {
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const opener = document.activeElement as HTMLElement | null;
    dialog.current?.showModal();
    return () => { opener?.focus(); };
  }, []);
  return <dialog ref={dialog} className="editor-dialog" aria-labelledby="dialog-title" onCancel={event => { event.preventDefault(); onClose(); }}
    onClick={event => { if (event.target === event.currentTarget) onClose(); }}>
    <div className="dialog-inner">
      <header><span className="dialog-logo">a</span><IconButton icon="close" label="Close dialog" onClick={onClose} /></header>
      <h2 id="dialog-title">{title}</h2>
      {children}
      <button className="standard-button primary-button" onClick={onClose}>Back to document</button>
    </div>
  </dialog>;
}

export default function App() {
  const [workspace, setWorkspace] = useState<WorkspaceId>('vector');
  const [states, setStates] = useState<Record<WorkspaceId, WorkspaceState>>(() => ({
    sketch: initialState('sketch'), vector: initialState('vector'), pixel: initialState('pixel'), layout: initialState('layout'),
  }));
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [dialog, setDialog] = useState<string | null>(null);
  const [toast, setToast] = useState('');
  const [hiddenChrome, setHiddenChrome] = useState(false);
  const [reducedMotion, setReducedMotion] = useState(false);
  const toastTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const state = states[workspace];
  const config = workspaces[workspace];
  const patch = useCallback((update: Partial<WorkspaceState>) => {
    setStates(previous => ({ ...previous, [workspace]: { ...previous[workspace], ...update } }));
  }, [workspace]);
  const setValue = (key: string, value: string) => setStates(previous => ({
    ...previous, [workspace]: { ...previous[workspace], values: { ...previous[workspace].values, [key]: value } },
  }));
  const toggle = (key: string) => setStates(previous => {
    const current = previous[workspace];
    return { ...previous, [workspace]: { ...current, toggles: current.toggles.includes(key) ? current.toggles.filter(item => item !== key) : [...current.toggles, key] } };
  });
  const feedback = useCallback((message: string) => {
    setToast(message);
    if (toastTimer.current) clearTimeout(toastTimer.current);
    toastTimer.current = setTimeout(() => setToast(''), 4200);
  }, []);
  useEffect(() => () => { if (toastTimer.current) clearTimeout(toastTimer.current); }, []);
  const openDialog = (title: string) => { setMenu(null); setDialog(title); };
  const switchWorkspace = (id: WorkspaceId) => {
    setWorkspace(id); setMenu(null); setDialog(null); setToast('');
  };
  const onPanel = (name: string) => {
    if (state.panels.includes(name)) patch({ panels: state.panels.filter(panel => panel !== name), pinned: state.pinned.filter(panel => panel !== name) });
    else patch({ panels: [...state.panels.filter(panel => state.pinned.includes(panel)), name] });
  };
  const onPin = (name: string) => patch({ pinned: state.pinned.includes(name) ? state.pinned.filter(panel => panel !== name) : [...state.pinned, name] });
  const fit = () => patch({ zoom: 1, pan: { x: 0, y: 0 } });
  const showMenu = (event: React.MouseEvent<HTMLElement>, title: string, items: MenuItem[]) => {
    const rect = event.currentTarget.getBoundingClientRect();
    setMenu(current => current?.title === title ? null : { title, x: Math.max(8, Math.min(window.innerWidth - 250, rect.left)), y: rect.bottom + 8, items });
  };
  useEffect(() => {
    const pointer = (event: PointerEvent) => {
      if (!(event.target as HTMLElement).closest('[data-popover], [data-menu-trigger]')) setMenu(null);
    };
    const key = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { setMenu(null); setDialog(null); setHiddenChrome(false); }
      const target = event.target as HTMLElement;
      if (target.matches('input, select, textarea') || dialog) return;
      if (event.key === 'Tab' && event.target === document.body) return;
      if ((event.ctrlKey || event.metaKey) && event.key === '0') {
        event.preventDefault(); patch({ zoom: 1, pan: { x: 0, y: 0 } });
      }
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'z') {
        event.preventDefault(); feedback('There are no document edits to undo.');
      }
    };
    document.addEventListener('pointerdown', pointer);
    document.addEventListener('keydown', key);
    return () => { document.removeEventListener('pointerdown', pointer); document.removeEventListener('keydown', key); };
  }, [dialog, patch, feedback]);
  useEffect(() => {
    if (!menu) return;
    const first = document.querySelector<HTMLButtonElement>('[data-popover] [role="menuitem"]');
    first?.focus();
  }, [menu]);
  const viewItems: MenuItem[] = [
    { label: 'Fit document', icon: 'zoom', action: fit, shortcut: '⌘0' },
    { label: 'Zoom in', icon: 'plus', action: () => patch({ zoom: Math.min(2.5, state.zoom + .15) }) },
    { label: 'Zoom out', icon: 'minus', action: () => patch({ zoom: Math.max(.75, state.zoom - .15) }) },
    { label: 'Show grid', icon: 'grid', checked: state.toggles.includes('grid'), action: () => toggle('grid') },
    { label: 'Hide interface', icon: 'eye', action: () => setHiddenChrome(true) },
  ];
  const docItems: MenuItem[] = [
    { label: 'New document…', icon: 'plus', action: () => openDialog('New document') },
    { label: 'Open document…', icon: 'pages', action: () => openDialog('Open document') },
    { label: 'Save document…', icon: 'layers', action: () => openDialog('Save document') },
    { label: 'Export…', icon: 'export', action: () => openDialog('Export document') },
    { label: 'Preferences…', icon: 'settings', action: () => openDialog('Preferences') },
    { label: 'About this recreation', icon: 'help', action: () => openDialog('Made for exploration') },
  ];
  const actions: PanelActions = { state, workspace, patch, setValue, toggle, feedback, openDialog };
  return <main className={`editor-scroll ${reducedMotion ? 'reduce-motion' : ''}`} style={{ '--persona-accent': config.accent } as CSSProperties}>
    <div className={`editor-shell workspace-${workspace} ${hiddenChrome ? 'chrome-hidden' : ''}`}>
      <DocumentCanvas key={workspace} workspace={workspace} state={state} patch={patch} />
      <div className="editor-chrome">
        <aside className="tool-rail chrome-surface" aria-label={`${config.label} tools`}>
          <button className="affinity-logo" aria-label="Document menu" data-menu-trigger onClick={event => showMenu(event, 'Document', docItems)}>a</button>
          <div className="tool-list">
            {config.tools.map(tool => <IconButton key={tool.id} icon={tool.icon} label={tool.label} active={state.selectedTool === tool.id} onClick={() => patch({ selectedTool: tool.id })} />)}
          </div>
          <div className="tool-rail-footer">
            <IconButton icon="more" label="More tools" data-menu-trigger onClick={event => showMenu(event, 'More tools', [
              { label: 'Deselect', icon: 'clear', action: () => { patch({ selectedLayer: '' }); feedback('Selection cleared.'); } },
              { label: 'Reset tool', icon: 'reset', action: () => patch({ selectedTool: config.selected }) },
              { label: 'Keyboard & interface help', icon: 'help', action: () => openDialog('Made for exploration') },
            ])} />
            <IconButton icon="clear" label="Deselect all" onClick={() => { patch({ selectedLayer: '' }); feedback('Selection cleared.'); }} />
            <IconButton icon="trash" label="Delete selection" onClick={() => feedback('The reference artwork is read-only. There are no edits to delete.')} />
          </div>
        </aside>
        <header className="persona-bar chrome-surface">
          <nav className="persona-tabs" aria-label="Workspaces">
            {personaOrder.map(id => <button key={id} className={`persona-tab ${workspace === id ? 'selected' : ''}`} aria-pressed={workspace === id} style={{ '--tab-accent': workspaces[id].accent } as CSSProperties} onClick={() => switchWorkspace(id)}>
              <Icon name={id} size={16} /><span>{workspaces[id].label}</span>
            </button>)}
          </nav>
          <button className="canva-button" onClick={() => openDialog('Canva AI')}><Icon name="ai" size={16} /><span>Canva AI</span></button>
          <IconButton icon="more" label="Workspace options" small data-menu-trigger onClick={event => showMenu(event, 'Workspace', [
            { label: 'Reset workspace', icon: 'reset', action: () => { setStates(previous => ({ ...previous, [workspace]: initialState(workspace) })); feedback(`${config.label} workspace reset.`); } },
            { label: 'Close all panels', icon: 'close', action: () => patch({ panels: [], pinned: [] }) },
            { label: 'Restore default panel', icon: 'layers', action: () => patch({ panels: [config.panel], pinned: [] }) },
          ])} />
          <span className="toolbar-divider" />
          <IconButton icon="pages" label="Document options" className="top-studio" active data-menu-trigger onClick={event => showMenu(event, 'Document', docItems)} />
          <IconButton icon="grid" label="View options" className="top-studio" active={state.toggles.includes('grid')} data-menu-trigger onClick={event => showMenu(event, 'View', viewItems)} />
          <IconButton icon="snap" label="Snapping" className="top-studio" active={state.toggles.includes('snapping')} onClick={() => toggle('snapping')} />
          <IconButton icon="menu" label="Main menu" data-menu-trigger onClick={event => showMenu(event, 'Main menu', [
            ...viewItems.slice(0, 1),
            { label: 'Undo', icon: 'undo', action: () => feedback('There are no document edits to undo.'), shortcut: '⌘Z' },
            { label: 'Redo', icon: 'redo', action: () => feedback('There are no document edits to redo.'), shortcut: '⇧⌘Z' },
            ...docItems.slice(3),
          ])} />
        </header>
        <aside className="studio-rail chrome-surface" aria-label="Studio panels">
          <button className="studio-colour" aria-label="Open colour panel" data-tooltip="Colour" onClick={() => onPanel('Colour')}><span style={{ background: state.values.colour }} /></button>
          {workspace === 'layout' && <IconButton icon="pages" label="Pages studio" active={state.panels.includes('Pages')} onClick={() => onPanel('Pages')} />}
          {studios.filter(studio => studio.id !== 'Colour').map(studio => <IconButton key={studio.id} icon={studio.icon} label={`${studio.label} studio`} active={state.panels.includes(studio.id)} onClick={() => onPanel(studio.id)} />)}
          <span className="rail-divider" />
          <IconButton icon="more" label="More studios" data-menu-trigger onClick={event => showMenu(event, 'Studios', [
            { label: 'Assets', icon: 'assets', action: () => onPanel('Assets') },
            { label: 'Close all panels', icon: 'close', action: () => patch({ panels: [], pinned: [] }) },
            { label: 'Reset studios', icon: 'reset', action: () => patch({ panels: [config.panel], pinned: [] }) },
          ])} />
          <IconButton icon="help" label="Interface help" onClick={() => openDialog('Made for exploration')} />
        </aside>
        <WorkspacePanels {...actions} onPanel={onPanel} onPin={onPin} />
        {workspace === 'vector' && <div className="node-toolbar chrome-surface" aria-label="Node tool actions">
          <div className="mini-grip" />
          {[
            ['node', 'Sharp node'], ['curve', 'Smooth node'], ['circle', 'Smart node'],
            ['transform', 'Break curve'], ['vector', 'Join curves'], ['stroke', 'Close curve'],
          ].map(([icon, label]) => <IconButton key={label} icon={icon} label={label} active={state.values.nodeAction === label} small onClick={() => setValue('nodeAction', label)} />)}
          <button className="stroke-indicator" aria-label="Open stroke controls" onClick={() => onPanel('Stroke')}><Icon name="eye" size={22} /><span>{state.values.strokeWidth} pt</span></button>
          <IconButton icon="more" label="More node options" small onClick={() => feedback('Select a node operation above. Path editing is not enabled.')} />
        </div>}
        {workspace === 'sketch' && <div className="sketch-quick-controls chrome-surface">
          <div className="mini-grip" />
          <input aria-label="Quick brush width" type="range" min="1" max="500" value={state.values.brushSize} onChange={event => setValue('brushSize', event.target.value)} />
          <button className="quick-brush-colour" aria-label="Open brush colour" style={{ background: state.values.colour }} onClick={() => onPanel('Colour')} />
          <input aria-label="Quick brush opacity" type="range" min="0" max="100" value={state.values.brushOpacity} onChange={event => setValue('brushOpacity', event.target.value)} />
          <IconButton icon="more" label="Brush settings" small onClick={() => onPanel('Brushes')} />
        </div>}
        {workspace !== 'sketch' && <button className="quick-colour-ring" aria-label="Quick colour" data-tooltip="Quick colour" onClick={() => onPanel('Colour')}><span style={{ background: state.values.colour }} /></button>}
        {workspace === 'layout' && <footer className="text-context-bar"><TextControls {...actions} compact /></footer>}
      </div>
      {hiddenChrome && <button className="restore-interface chrome-surface" onClick={() => setHiddenChrome(false)}><Icon name="menu" size={18} />Show interface<span>Esc</span></button>}
      {menu && <div className="editor-popover chrome-surface" role="menu" aria-label={menu.title} data-popover style={{ left: menu.x, top: menu.y }}
        onKeyDown={event => {
          const items = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="menuitem"]'));
          const index = items.indexOf(document.activeElement as HTMLButtonElement);
          if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
            event.preventDefault(); items[(index + (event.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length]?.focus();
          }
        }}>
        <span className="popover-title">{menu.title}</span>
        {menu.items.map(item => <button key={item.label} role="menuitem" onClick={() => { setMenu(null); item.action(); }}><Icon name={item.checked ? 'check' : item.icon} size={16} /><span>{item.label}</span>{item.shortcut && <kbd>{item.shortcut}</kbd>}</button>)}
      </div>}
      {toast && <div className="editor-toast" role="status"><Icon name="help" size={17} /><span>{toast}</span><button aria-label="Dismiss notification" onClick={() => setToast('')}><Icon name="close" size={15} /></button></div>}
      {dialog && <Modal title={dialog} onClose={() => setDialog(null)}>
        {dialog === 'Made for exploration' ? <>
          <p>A familiar canvas. Four creative workspaces.</p>
          <p className="dialog-detail">Switch between Sketch, Vector, Pixel and Layout. Explore tools, studio panels and controls, or navigate around the reference artwork.</p>
          <div className="dialog-info"><Icon name="layers" size={20} /><span>This is an interactive UI recreation, not an editing app. Artwork stays unchanged and nothing is uploaded or saved.</span></div>
          <p className="dialog-shortcuts"><kbd>Esc</kbd> Close overlays <kbd>⌘ / Ctrl + 0</kbd> Fit document</p>
        </> : dialog === 'Preferences' ? <>
          <p>Interface preferences</p>
          <label className="preference-row">Reduce interface motion<input type="checkbox" checked={reducedMotion} onChange={event => setReducedMotion(event.target.checked)} /></label>
          <p className="dialog-detail">Workspace settings are kept while you explore. Reloading restores the reference defaults.</p>
        </> : <>
          <p>{dialog === 'Canva AI' ? 'A little space for big ideas.' : config.title}</p>
          <div className="dialog-info"><Icon name={dialog === 'Canva AI' ? 'ai' : 'pages'} size={24} /><span>{dialog === 'Canva AI' ? 'AI generation is not connected in this interface recreation. No prompts or artwork are sent to a service.' : 'Document creation, file operations and export are not enabled in this UI recreation. The supplied reference artwork is read-only.'}</span></div>
          <p className="dialog-detail">All four workspaces and their interface controls are available to explore.</p>
        </>}
      </Modal>}
    </div>
  </main>;
}
