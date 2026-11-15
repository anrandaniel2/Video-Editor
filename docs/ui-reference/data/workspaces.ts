import vector from '../assets/artwork/vector.webp';
import pixel from '../assets/artwork/pixel.webp';
import layout from '../assets/artwork/layout.webp';
import sketch from '../assets/artwork/sketch.svg';

export type WorkspaceId = 'sketch' | 'vector' | 'pixel' | 'layout';
export type Tool = { id: string; icon: string; label: string };
export const tool = (id: string, icon: string, label: string): Tool => ({ id, icon, label });
const shared = [
  tool('hand', 'hand', 'View Tool'),
  tool('zoom', 'zoom', 'Zoom Tool'),
];
export const workspaces: Record<WorkspaceId, { label: string; accent: string; artwork: string; title: string; tools: Tool[]; panel: string; selected: string }> = {
  sketch: {
    label: 'Sketch', accent: '#e8f633', artwork: sketch, title: 'Workout Meals — Logo studies',
    panel: 'Brushes', selected: 'pencil',
    tools: [
      tool('move', 'move', 'Move Tool'), tool('pencil', 'pencil', 'Pencil Tool'),
      tool('brush', 'brush', 'Paint Brush Tool'), tool('eraser', 'eraser', 'Erase Brush Tool'),
      tool('fill', 'fill', 'Flood Fill Tool'), tool('rectangle', 'rectangle', 'Rectangle Tool'),
      tool('select', 'marquee', 'Selection Tool'), ...shared,
    ],
  },
  vector: {
    label: 'Vector', accent: '#55e3f9', artwork: vector, title: 'Otherworld — Botanical forms',
    panel: 'Layers', selected: 'node',
    tools: [
      tool('move', 'move', 'Move Tool'), tool('select', 'select', 'Artboard Tool'),
      tool('node', 'node', 'Node Tool'), tool('corner', 'curve', 'Corner Tool'),
      tool('contour', 'contrast', 'Contour Tool'), tool('pen', 'pen', 'Pen Tool'),
      tool('pencil', 'pencil', 'Pencil Tool'), tool('brush', 'brush', 'Vector Brush Tool'),
      tool('fill', 'fill', 'Fill Tool'), tool('rectangle', 'rectangle', 'Rectangle Tool'),
      tool('ellipse', 'circle', 'Ellipse Tool'), tool('text', 'text', 'Artistic Text Tool'),
      tool('image', 'image', 'Place Image Tool'), tool('point', 'wand', 'Point Transform Tool'),
      tool('measure', 'pencil', 'Measure Tool'), tool('dropper', 'dropper', 'Colour Picker Tool'), ...shared,
    ],
  },
  pixel: {
    label: 'Pixel', accent: '#dd9cfb', artwork: pixel, title: 'Rouge — Portrait study',
    panel: 'Curves', selected: 'move',
    tools: [
      tool('move', 'move', 'Move Tool'), tool('crop', 'crop', 'Crop Tool'),
      tool('select', 'select', 'Selection Brush Tool'), tool('marquee', 'marquee', 'Rectangular Marquee Tool'),
      tool('wand', 'wand', 'Flood Select Tool'), tool('brush', 'brush', 'Paint Brush Tool'),
      tool('pencil', 'pencil', 'Pixel Tool'), tool('eraser', 'eraser', 'Erase Brush Tool'),
      tool('heal', 'heal', 'Healing Brush Tool'), tool('clone', 'clone', 'Clone Brush Tool'),
      tool('fill', 'fill', 'Flood Fill Tool'), tool('pen', 'pen', 'Pen Tool'),
      tool('rectangle', 'rectangle', 'Rectangle Tool'), tool('text', 'text', 'Artistic Text Tool'),
      tool('dropper', 'dropper', 'Colour Picker Tool'), ...shared,
    ],
  },
  layout: {
    label: 'Layout', accent: '#ffab89', artwork: layout, title: 'Showcase — Issue 04',
    panel: 'Pages', selected: 'text',
    tools: [
      tool('move', 'move', 'Move Tool'), tool('node', 'node', 'Node Tool'),
      tool('text', 'text', 'Frame Text Tool'), tool('table', 'table', 'Table Tool'),
      tool('artistic', 'text', 'Artistic Text Tool'), tool('pen', 'pen', 'Pen Tool'),
      tool('rectangle', 'rectangle', 'Rectangle Tool'), tool('frame', 'frame', 'Picture Frame Rectangle Tool'),
      tool('ellipse', 'circle', 'Picture Frame Ellipse Tool'), tool('image', 'image', 'Place Image Tool'),
      tool('point', 'wand', 'Point Transform Tool'), tool('brush', 'brush', 'Vector Brush Tool'),
      tool('transparency', 'contrast', 'Transparency Tool'), tool('dropper', 'dropper', 'Colour Picker Tool'), ...shared,
    ],
  },
};

export const studios = [
  tool('Colour', 'circle', 'Colour'), tool('Swatches', 'grid', 'Swatches'),
  tool('Stroke', 'stroke', 'Stroke'), tool('Text', 'text', 'Text'),
  tool('Paragraph', 'list', 'Paragraph'), tool('Layers', 'layers', 'Layers'),
  tool('Brushes', 'brush', 'Brushes'), tool('Curves', 'contrast', 'Adjustments'),
  tool('Effects', 'effects', 'Layer Effects'), tool('Transform', 'transform', 'Transform'),
  tool('Navigator', 'navigator', 'Navigator'), tool('History', 'history', 'History'),
  tool('Export', 'export', 'Export'),
];

export type Point = { x: number; y: number };
export const defaultPoints: Point[] = [{ x: 0, y: 100 }, { x: 22, y: 80 }, { x: 66, y: 30 }, { x: 100, y: 0 }];
export type WorkspaceState = {
  selectedTool: string;
  panels: string[];
  pinned: string[];
  selectedLayer: string;
  collapsed: string[];
  hidden: string[];
  locked: string[];
  page: number;
  brush: string;
  search: string;
  category: string;
  values: Record<string, string>;
  toggles: string[];
  points: Point[];
  zoom: number;
  pan: { x: number; y: number };
};
export function initialState(id: WorkspaceId): WorkspaceState {
  return {
    selectedTool: workspaces[id].selected, panels: [workspaces[id].panel], pinned: [],
    selectedLayer: 'stem', collapsed: [], hidden: ['sketch'], locked: ['master'],
    page: 8, brush: 'Natural Pencil 4B', search: '', category: 'Sketching',
    values: { opacity: '100', blend: 'Normal', size: '14', brushSize: '24', brushOpacity: '100', colour: '#f59ac9', channel: 'Master', colourSpace: 'RGB', font: 'Instrument Serif', fontStyle: 'Regular', strokeWidth: '0.7', alignment: 'align' },
    toggles: ['snapping'], points: defaultPoints.map(p => ({ ...p })), zoom: 1, pan: { x: 0, y: 0 },
  };
}
