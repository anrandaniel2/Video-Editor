import { useRef, useState } from 'react';
import { defaultPoints, workspaces, type Point, type WorkspaceId, type WorkspaceState } from '../../data/workspaces';
import page2 from '../../assets/artwork/pages-2.webp';
import page4 from '../../assets/artwork/pages-4.webp';
import page6 from '../../assets/artwork/pages-6.webp';
import { Icon } from './EditorIcon';
import { IconButton, NumberField, Panel, SelectField } from './Controls';

export type PanelActions = {
  state: WorkspaceState;
  workspace: WorkspaceId;
  patch: (state: Partial<WorkspaceState>) => void;
  setValue: (key: string, value: string) => void;
  toggle: (key: string) => void;
  feedback: (message: string) => void;
  openDialog: (title: string) => void;
};

const layerRows = [
  { id: 'master', name: 'Master A', kind: 'text', depth: 0 },
  { id: 'text', name: 'Text', kind: 'text', depth: 0 },
  { id: 'illustration', name: 'Illustration', kind: 'image', depth: 0, group: true },
  { id: 'flower', name: '2', kind: 'brush', depth: 1, group: true, parent: 'illustration' },
  { id: 'stem', name: 'Stem', kind: 'pen', depth: 2, parent: 'flower' },
  { id: 'highlights', name: 'Highlights', kind: 'circle', depth: 3, parent: 'flower' },
  { id: 'shadows', name: 'Shadows', kind: 'circle', depth: 3, parent: 'flower' },
  { id: 'stripes', name: 'Stripes', kind: 'stroke', depth: 2, parent: 'flower' },
  { id: 'gradient', name: 'Gradient', kind: 'curve', depth: 2, parent: 'flower' },
  { id: 'mushroom', name: 'Mushroom', kind: 'image', depth: 1, parent: 'illustration', group: true },
  { id: 'sketch', name: 'Sketch', kind: 'pencil', depth: 0 },
  { id: 'pixel', name: 'Pixel', kind: 'image', depth: 0, group: true },
  { id: 'brightness', name: 'Brightness / Contrast', kind: 'contrast', depth: 1, parent: 'pixel' },
  { id: 'curves', name: 'Curves Adjustment', kind: 'curve', depth: 1, parent: 'pixel' },
  { id: 'mask', name: 'Mask', kind: 'mask', depth: 1, parent: 'pixel' },
];

function Layers({ state, patch, setValue, feedback }: PanelActions) {
  const toggleList = (key: 'hidden' | 'locked' | 'collapsed', id: string) =>
    patch({ [key]: state[key].includes(id) ? state[key].filter(item => item !== id) : [...state[key], id] });
  return <>
    <div className="panel-toolbar">
      <IconButton icon="more" label="Layer actions" small onClick={() => feedback('Select a layer to inspect its controls. Document editing is not enabled.')} />
      <IconButton icon="mask" label="Add mask" small onClick={() => feedback('Masks are preview-only in this interface.')} />
      <IconButton icon="duplicate" label="Duplicate layer" small onClick={() => feedback('Layer duplication is not enabled in this UI replica.')} />
      <IconButton icon="layers" label="Add layer group" small onClick={() => feedback('Layer creation is not enabled in this UI replica.')} />
      <IconButton icon="trash" label="Delete layer" small onClick={() => feedback('The reference document cannot be modified.')} />
    </div>
    <div className="layer-options">
      <NumberField label="Layer opacity" value={state.values.opacity} onChange={v => setValue('opacity', v)} suffix="%" />
      <SelectField label="Blend mode" value={state.values.blend} options={['Normal', 'Multiply', 'Screen', 'Overlay', 'Soft Light', 'Colour']} onChange={v => setValue('blend', v)} />
      <IconButton icon="settings" label="Layer options" small onClick={() => toggleList('locked', state.selectedLayer)} />
    </div>
    <div className="layers-list">
      {layerRows.filter(row => {
        if (row.parent && state.collapsed.includes(row.parent)) return false;
        if (['stem', 'highlights', 'shadows', 'stripes', 'gradient', 'mushroom', 'flower'].includes(row.id) && state.collapsed.includes('illustration')) return false;
        return true;
      }).map(row => <div key={row.id} className={`layer-row ${state.selectedLayer === row.id ? 'selected' : ''} ${state.hidden.includes(row.id) ? 'layer-hidden' : ''}`}>
        <div className="layer-indent" style={{ width: 7 + row.depth * 5 }} />
        {row.group
          ? <button className={`layer-disclosure ${!state.collapsed.includes(row.id) ? 'expanded' : ''}`} aria-label={`${state.collapsed.includes(row.id) ? 'Expand' : 'Collapse'} ${row.name}`} aria-expanded={!state.collapsed.includes(row.id)} onClick={() => toggleList('collapsed', row.id)}><Icon name="chevron" size={11} /></button>
          : <span className="layer-disclosure-spacer" />}
        <button className="layer-select" aria-label={`Select ${row.name} layer`} aria-pressed={state.selectedLayer === row.id} onClick={() => patch({ selectedLayer: row.id })}>
          <span className={`layer-thumb thumb-${row.id}`}><Icon name={row.kind} size={21} /></span>
          <span className="layer-name">{row.name}</span>
        </button>
        <button className={`layer-lock ${state.locked.includes(row.id) ? 'locked' : ''}`} aria-label={`${state.locked.includes(row.id) ? 'Unlock' : 'Lock'} ${row.name}`} onClick={() => toggleList('locked', row.id)}><Icon name={state.locked.includes(row.id) ? 'lock' : 'unlock'} size={11} /></button>
        <button className="layer-eye" aria-label={`${state.hidden.includes(row.id) ? 'Show' : 'Hide'} ${row.name}`} aria-pressed={!state.hidden.includes(row.id)} onClick={() => toggleList('hidden', row.id)}><Icon name={state.hidden.includes(row.id) ? 'hidden' : 'eye'} size={13} /></button>
      </div>)}
    </div>
    <div className="panel-footer"><span>{layerRows.length} layers</span><span>UI preview</span></div>
  </>;
}

function CurveGraph({ state, patch }: PanelActions) {
  const [selected, setSelected] = useState(1);
  const [dragging, setDragging] = useState<number | null>(null);
  const graph = useRef<SVGSVGElement>(null);
  const changePoint = (index: number, next: Point) => {
    const points = state.points.map((point, i) => i === index ? {
      x: Math.max(i === 0 ? 0 : state.points[i - 1].x + 1, Math.min(i === state.points.length - 1 ? 100 : state.points[i + 1].x - 1, next.x)),
      y: Math.max(0, Math.min(100, next.y)),
    } : point);
    patch({ points });
  };
  const point = state.points[Math.min(selected, state.points.length - 1)];
  const polygon = state.points.map(p => `${p.x * 2},${p.y * 2}`).join(' ');
  const histogram = (seed: number) => `M0 200 ${Array.from({ length: 101 }, (_, i) => {
    const peak = Math.exp(-((i - seed) ** 2) / 300) * 65 + Math.exp(-((i - 76) ** 2) / 30) * 85;
    const n = ((i * 17 + seed * 13) % 29) / 29;
    return `L${i * 2} ${200 - peak * (0.1 + n)}`;
  }).join(' ')} L200 200Z`;
  return <div className="curve-editor">
    <div className="curve-graph-wrap">
      <svg ref={graph} className="curve-graph" viewBox="0 0 200 200" aria-label="Curve graph" role="group"
        onPointerMove={event => {
          if (dragging === null || !graph.current) return;
          const rect = graph.current.getBoundingClientRect();
          changePoint(dragging, { x: (event.clientX - rect.left) / rect.width * 100, y: (event.clientY - rect.top) / rect.height * 100 });
        }}
        onPointerUp={() => setDragging(null)} onPointerCancel={() => setDragging(null)}>
        <rect width="200" height="200" fill="#191b1a" />
        {[25, 50, 75, 100, 125, 150, 175].map(n => <path key={n} d={`M${n} 0V200M0 ${n}H200`} stroke="#4e514f" strokeWidth=".5" />)}
        <path d={histogram(72)} fill="#c73536" fillOpacity=".6" stroke="#e84949" strokeWidth=".7" />
        <path d={histogram(55)} fill="#2d9240" fillOpacity=".45" stroke="#6bb047" strokeWidth=".7" />
        <path d={histogram(43)} fill="#2743b8" fillOpacity=".55" stroke="#4361e1" strokeWidth=".7" />
        <path d="M0 200 200 0" stroke="#747674" strokeWidth=".7" />
        <polyline points={polygon} fill="none" stroke="#fafafa" strokeWidth="1.4" />
        {state.points.map((p, index) => <circle key={index} cx={p.x * 2} cy={p.y * 2} r={index === selected ? 4 : 3} fill={index === selected ? '#2e98ff' : '#fff'} stroke="#fff" strokeWidth=".7"
          role="slider" tabIndex={0} aria-label={`Curve point ${index + 1}`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(100 - p.y)} aria-valuetext={`Input ${Math.round(p.x)}, output ${Math.round(100 - p.y)}`}
          onFocus={() => setSelected(index)}
          onPointerDown={event => { event.stopPropagation(); setSelected(index); setDragging(index); event.currentTarget.setPointerCapture(event.pointerId); }}
          onKeyDown={event => {
            if (!['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) return;
            event.preventDefault();
            changePoint(index, { x: p.x + (event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0), y: p.y + (event.key === 'ArrowDown' ? 1 : event.key === 'ArrowUp' ? -1 : 0) });
          }} />)}
      </svg>
      <div className="curve-side">
        <IconButton icon="reset" label="Reset curve" small onClick={() => { patch({ points: defaultPoints.map(p => ({ ...p })) }); setSelected(1); }} />
        <label>X<NumberField label="Curve input" value={String(Math.round(point.x))} onChange={v => changePoint(selected, { ...point, x: Number(v) })} /></label>
        <label>Y<NumberField label="Curve output" value={String(Math.round(100 - point.y))} onChange={v => changePoint(selected, { ...point, y: 100 - Number(v) })} /></label>
      </div>
    </div>
    <div className="curve-hint">Drag a point to preview the curve</div>
  </div>;
}

function Curves(props: PanelActions) {
  const { state, setValue, patch } = props;
  return <>
    <div className="curves-options">
      <div>
        <SelectField label="Colour space" value={state.values.colourSpace} options={['RGB', 'CMYK', 'LAB', 'Greyscale']} onChange={v => setValue('colourSpace', v)} />
        <span className="field-caption">Colour space</span>
        <SelectField label="Curve channel" value={state.values.channel} options={['Master', 'Red', 'Green', 'Blue', 'Alpha']} onChange={v => setValue('channel', v)} />
        <span className="field-caption">Channels</span>
      </div>
      <div className="dial-control"><div className="dial"><span>{Math.round(state.points[1]?.x || 0)}</span></div><span>Input minimum</span></div>
      <div className="dial-control"><div className="dial opacity-dial"><NumberField label="Adjustment opacity" value={state.values.opacity} onChange={v => setValue('opacity', v)} /></div><span>Opacity</span></div>
    </div>
    <div className="panel-toolbar curve-actions">
      <IconButton icon="reset" label="Reset adjustment" small onClick={() => { patch({ points: defaultPoints.map(p => ({ ...p })) }); setValue('opacity', '100'); }} />
      <IconButton icon="eye" label="Preview adjustment" active={state.toggles.includes('curvePreview')} small onClick={() => props.toggle('curvePreview')} />
      <IconButton icon="trash" label="Remove adjustment" small onClick={() => props.feedback('The reference adjustment cannot be removed in this UI replica.')} />
      <IconButton icon="plus" label="Add adjustment" small onClick={() => props.feedback('Adjustment creation is not enabled. The curve graph is interactive.')} />
    </div>
    <CurveGraph {...props} />
    <p className="panel-note">Curve controls are a preview. The portrait stays unchanged.</p>
  </>;
}

function Pages({ state, patch, feedback, toggle }: PanelActions) {
  const spreads = [
    { image: page2, page: 2, title: 'Nora Toth' },
    { image: page4, page: 4, title: 'Paloma Rincon' },
    { image: page6, page: 6, title: 'Marrak' },
    { image: workspaces.layout.artwork, page: 8, title: 'Isabela Humphrey' },
  ];
  return <>
    <div className="pages-navigation">
      <IconButton icon="chevron" className="flip" label="Previous spread" small onClick={() => patch({ page: Math.max(2, state.page - 2) })} />
      <span>Pages</span>
      <IconButton icon="chevron" label="Next spread" small onClick={() => patch({ page: Math.min(8, state.page + 2) })} />
    </div>
    <div className="panel-toolbar">
      <IconButton icon="plus" label="Add page" small onClick={() => feedback('Page creation is not enabled in this UI replica.')} />
      <IconButton icon="duplicate" label="Duplicate page" small onClick={() => feedback('The reference spread cannot be duplicated.')} />
      <IconButton icon="trash" label="Delete page" small onClick={() => feedback('The reference spread cannot be deleted.')} />
      <IconButton icon="more" label="Page options" small onClick={() => toggle('managePages')} />
    </div>
    <div className="page-list">
      {spreads.map(spread => <button className={`page-thumbnail ${state.page === spread.page ? 'selected-page' : ''}`} key={spread.page} aria-label={`Select pages ${spread.page}, ${spread.page + 1}: ${spread.title}`} aria-pressed={state.page === spread.page} onClick={() => patch({ page: spread.page })}>
        <img src={spread.image} alt={`${spread.title} editorial spread`} />
        <span>Pages {spread.page},{spread.page + 1}</span>
      </button>)}
    </div>
    <button className="disclosure-row" aria-expanded={state.toggles.includes('managePages')} onClick={() => toggle('managePages')}>Manage Pages<Icon name="chevron" size={14} /></button>
    {state.toggles.includes('managePages') && <div className="expanded-section">8 pages · 4 spreads<br /><span>Facing pages · Portrait</span></div>}
    <button className="disclosure-row" aria-expanded={state.toggles.includes('sections')} onClick={() => toggle('sections')}>Sections<Icon name="chevron" size={14} /></button>
    {state.toggles.includes('sections') && <div className="expanded-section">Section 1 — Showcase<br /><span>Pages 2–9</span></div>}
  </>;
}

const brushNames = ['Natural Pencil 6B', 'Natural Pencil 2B', 'Natural Pencil 4B', 'Compressed Charcoal', 'Dense Graphite', 'Round Dry Bristle', 'Soft Chalk', 'Fine Ink'];
function Brushes({ state, patch, setValue }: PanelActions) {
  const brushes = brushNames.filter(name => name.toLowerCase().includes(state.search.toLowerCase()) && (state.category !== 'Inking' || name.includes('Ink')));
  return <>
    <label className="brush-search"><Icon name="search" size={13} /><input aria-label="Search brushes" placeholder="Find a brush" value={state.search} onChange={event => patch({ search: event.target.value })} />{state.search && <button aria-label="Clear brush search" onClick={() => patch({ search: '' })}><Icon name="close" size={12} /></button>}</label>
    <SelectField label="Brush category" value={state.category} options={['Sketching', 'Pencils', 'Inking', 'All Brushes']} onChange={category => patch({ category })} />
    <div className="brush-list">
      {brushes.map((name, i) => <button key={name} className={`brush-preset ${state.brush === name ? 'selected-brush' : ''}`} aria-label={`Select ${name}`} aria-pressed={state.brush === name} onClick={() => patch({ brush: name })}>
        <svg viewBox="0 0 200 32" aria-hidden="true"><defs><linearGradient id={`brush-${i}`}><stop stopColor="#fff" stopOpacity=".05" /><stop offset=".4" stopColor="#eee" stopOpacity=".9" /><stop offset="1" stopColor="#fff" stopOpacity=".05" /></linearGradient></defs><path d="M3 23Q52 5 100 15T197 9" fill="none" stroke={`url(#brush-${i})`} strokeWidth={2 + (i % 5) * 2} strokeLinecap="round" />{i < 5 && <path d="M6 26Q52 7 105 19T197 12" fill="none" stroke="#aaa" strokeWidth=".6" />}</svg>
        <span>{name}</span>
      </button>)}
      {!brushes.length && <div className="empty-state">No brushes match “{state.search}”.<button onClick={() => { patch({ search: '', category: 'All Brushes' }); }}>Show all brushes</button></div>}
    </div>
    <div className="brush-controls">
      <label>Width<NumberField label="Brush width" value={state.values.brushSize} min={1} max={500} suffix="px" onChange={v => setValue('brushSize', v)} /></label>
      <input aria-label="Brush width slider" type="range" min="1" max="500" value={state.values.brushSize || 1} onChange={event => setValue('brushSize', event.target.value)} />
      <label>Opacity<NumberField label="Brush opacity" value={state.values.brushOpacity} suffix="%" onChange={v => setValue('brushOpacity', v)} /></label>
      <input aria-label="Brush opacity slider" type="range" min="0" max="100" value={state.values.brushOpacity || 0} onChange={event => setValue('brushOpacity', event.target.value)} />
    </div>
  </>;
}

const swatches = ['#f29aca', '#f94c72', '#fd7e29', '#ffce22', '#d6fb30', '#40bb83', '#55e3f9', '#278aff', '#8965ef', '#121313', '#8e9296', '#ffffff'];
function Colour({ state, setValue }: PanelActions) {
  return <div className="colour-content">
    <div className="colour-wheel"><div style={{ background: state.values.colour }} /></div>
    <div className="colour-value"><input aria-label="Choose colour" type="color" value={state.values.colour} onChange={event => setValue('colour', event.target.value)} /><span>{state.values.colour.toUpperCase()}</span><span className="muted">RGB/8</span></div>
    <div className="colour-swatches">{swatches.map(colour => <button key={colour} style={{ background: colour }} aria-label={`Select colour ${colour}`} aria-pressed={state.values.colour === colour} onClick={() => setValue('colour', colour)}>{state.values.colour === colour && <Icon name="check" size={15} style={{ color: ['#121313', '#278aff', '#8965ef'].includes(colour) ? 'white' : '#222' }} />}</button>)}</div>
    <p className="panel-note">Selected colour updates the interface swatch only.</p>
  </div>;
}

export function TextControls({ state, setValue, toggle, compact = false }: PanelActions & { compact?: boolean }) {
  return <div className={compact ? 'text-controls compact-text' : 'text-controls'}>
    <SelectField label="Font family" value={state.values.font} options={['Instrument Serif', 'Arial', 'Georgia', 'Helvetica', 'Times New Roman']} onChange={v => setValue('font', v)} />
    <SelectField label="Font style" value={state.values.fontStyle} options={['Regular', 'Medium', 'Bold', 'Italic']} onChange={v => setValue('fontStyle', v)} />
    <NumberField label="Font size" value={state.values.size} min={1} max={288} suffix="pt" onChange={v => setValue('size', v)} />
    {['bold', 'italic', 'underline'].map(name => <IconButton key={name} icon={name} label={`${name[0].toUpperCase() + name.slice(1)} text`} active={state.toggles.includes(name)} small onClick={() => toggle(name)} />)}
    <label className="text-colour" data-tooltip="Text colour"><span className="sr-only">Text colour</span><input aria-label="Text colour" type="color" value={state.values.colour} onChange={event => setValue('colour', event.target.value)} /></label>
    {['align', 'center', 'right', 'list'].map((name, index) => <IconButton key={name} icon={name} label={['Align left', 'Align centre', 'Align right', 'Bulleted list'][index]} active={state.values.alignment === name} small onClick={() => setValue('alignment', name)} />)}
  </div>;
}

function SecondaryPanel(props: PanelActions & { name: string }) {
  const { name, state, setValue, toggle, feedback, openDialog, patch, workspace } = props;
  if (name === 'Colour' || name === 'Swatches') return <Colour {...props} />;
  if (name === 'Text' || name === 'Paragraph') return <div className="secondary-content"><TextControls {...props} /><p className="panel-note">Formatting controls are preview-only.</p></div>;
  if (name === 'Stroke') return <div className="secondary-content">
    <label className="field-row">Width<NumberField label="Stroke width" value={state.values.strokeWidth} min={0} max={100} step={0.1} suffix="pt" onChange={v => setValue('strokeWidth', v)} /></label>
    <SelectField label="Stroke style" value={state.values.strokeStyle || 'Solid'} options={['Solid', 'Dashed', 'Dotted', 'None']} onChange={v => setValue('strokeStyle', v)} />
    <div className="stroke-preview" style={{ borderTopWidth: Math.max(1, Math.min(12, Number(state.values.strokeWidth))), borderTopStyle: state.values.strokeStyle === 'Dashed' ? 'dashed' : state.values.strokeStyle === 'Dotted' ? 'dotted' : 'solid', opacity: state.values.strokeStyle === 'None' ? 0 : 1 }} />
    <label className="field-row">Pressure<IconButton icon="curve" label="Pressure profile" active={state.toggles.includes('pressure')} onClick={() => toggle('pressure')} /></label>
  </div>;
  if (name === 'Effects') return <div className="secondary-content">{['Gaussian Blur', 'Outer Shadow', 'Inner Shadow', 'Outer Glow', 'Bevel / Emboss', 'Colour Overlay'].map(effect => <label className="effect-row" key={effect}>{effect}<input type="checkbox" checked={state.toggles.includes(effect)} onChange={() => toggle(effect)} /></label>)}<p className="panel-note">Effects do not modify the reference artwork.</p></div>;
  if (name === 'Transform') return <div className="secondary-content transform-fields">{['X', 'Y', 'Width', 'Height', 'Rotation', 'Shear'].map((label, i) => <label className="field-row" key={label}>{label}<NumberField label={`Transform ${label}`} value={state.values[`transform${label}`] ?? String(i === 2 ? 944 : i === 3 ? 707 : 0)} min={i < 2 ? -10000 : i > 3 ? -360 : 0} max={i > 3 ? 360 : 10000} suffix={i > 3 ? '°' : 'px'} onChange={v => setValue(`transform${label}`, v)} /></label>)}<p className="panel-note">Document transforms are preview-only.</p></div>;
  if (name === 'Navigator') return <div className="secondary-content">
    <img className="navigator-preview" src={workspaces[workspace].artwork} alt="Current document overview" />
    <label className="field-row">Zoom<span>{Math.round(state.zoom * 100)}%</span></label>
    <input aria-label="Document zoom" type="range" min="75" max="250" value={state.zoom * 100} onChange={event => patch({ zoom: Number(event.target.value) / 100 })} />
    <button className="standard-button" onClick={() => patch({ zoom: 1, pan: { x: 0, y: 0 } })}>Fit document</button>
  </div>;
  if (name === 'History') return <div className="secondary-content"><div className="history-entry"><Icon name="history" size={16} />Opened reference document<Icon name="check" size={14} /></div><p className="panel-note">No document edits. Interface changes are not added to history.</p><button className="standard-button" onClick={() => feedback('There are no document edits to clear.')}>Clear history</button></div>;
  if (name === 'Export') return <div className="secondary-content"><SelectField label="Export format" value={state.values.exportFormat || 'PNG'} options={['PNG', 'JPEG', 'SVG', 'PDF']} onChange={v => setValue('exportFormat', v)} /><p className="panel-note">Explore export options. File generation is not included in this UI replica.</p><button className="standard-button primary-button" onClick={() => openDialog('Export document')}>Export…</button></div>;
  return <div className="secondary-content"><p className="panel-note">No assets in this reference document.</p><button className="standard-button" onClick={() => feedback('Asset import is not enabled in this UI replica.')}>Import assets…</button></div>;
}

export function WorkspacePanels(props: PanelActions & { onPanel: (name: string) => void; onPin: (name: string) => void }) {
  return <div className="panels-layer">
    {props.state.panels.map((name, index) => <Panel key={`${props.workspace}-${name}`} name={name} pinned={props.state.pinned.includes(name)} onPin={() => props.onPin(name)} onClose={() => props.onPanel(name)}
      className={`studio-panel panel-${name.toLowerCase()} workspace-panel-${props.workspace} ${index > 0 ? 'extra-panel' : ''}`}>
      {name === 'Layers' ? <Layers {...props} /> : name === 'Curves' ? <Curves {...props} /> : name === 'Pages' ? <Pages {...props} /> : name === 'Brushes' ? <Brushes {...props} /> : <SecondaryPanel name={name} {...props} />}
    </Panel>)}
  </div>;
}
