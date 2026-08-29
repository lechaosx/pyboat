import {
  Background,
  Controls,
  Handle,
  MarkerType,
  MiniMap,
  Panel,
  Position,
  ReactFlow,
  useNodesState,
  type Edge,
  type Node,
  type NodeProps,
  type ReactFlowInstance,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import {
  createRef,
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useState,
} from "react";
import { createRoot } from "react-dom/client";
import {
  shaderGraphEdges,
  shaderGraphNodes,
  validateShaderGraph,
  type ShaderGraphControl,
  type ShaderGraphEdge,
  type ShaderGraphNode,
  type ShaderSocketType,
} from "./shader-graph";
import "./shader-graph.css";

type ShaderGraphValue = number | string;

type ShaderGraphBindings = {
  getValue: (key: string) => ShaderGraphValue;
  getDefaultValue: (key: string) => ShaderGraphValue;
  setValue: (key: string, value: ShaderGraphValue) => void;
  getDiagnostic: () => string;
  setDiagnostic: (diagnostic: string) => void;
};

export type ShaderGraphController = {
  sync: () => void;
  destroy: () => void;
};

type ShaderNodeData = {
  model: ShaderGraphNode;
  inputs: readonly ShaderGraphEdge[];
  muted: boolean;
  bindings: ShaderGraphBindings;
  onControl: (key: string, value: ShaderGraphValue) => void;
  onMute: (node: ShaderGraphNode, muted: boolean) => void;
};

type ShaderFlowNode = Node<ShaderNodeData, "shader">;

const socketColors: Record<ShaderSocketType, string> = {
  state: "#d7b26d",
  geometry: "#67bed8",
  field: "#d77da8",
  vector: "#a58ae8",
  mask: "#a8b6b9",
  color: "#72c99c",
};

const kindLabels: Record<ShaderGraphNode["kind"], string> = {
  input: "INPUT",
  physical: "PHYSICAL",
  intermediate: "VALUE",
  visual: "VISUAL",
  output: "OUTPUT",
};

const nodeWidth = 260;
const initialPositions: Record<string, { x: number; y: number }> = {
  seed_input: { x: 852, y: 666 },
  world_geometry_input: { x: 450, y: 832 },
  field_clocks_input: { x: 1254, y: 996 },
  world_clock_input: { x: 450, y: 617 },
  view_input: { x: 48, y: 2014 },
  fragment_input: { x: 450, y: 2014 },
  surface_coordinates: { x: 852, y: 861 },
  terrain_geometry: { x: 1656, y: 503 },
  coastal_relief: { x: 1254, y: 665 },
  tide: { x: 1656, y: 325 },
  pressure: { x: 1656, y: 1297 },
  temperature: { x: 1656, y: 1057 },
  moisture: { x: 1656, y: 854 },
  surface_wind: { x: 2058, y: 906 },
  surface_weather: { x: 2460, y: 1272 },
  celestial_state: { x: 2460, y: 1039 },
  regional_canopy: { x: 2862, y: 1745 },
  low_cloud_field: { x: 2862, y: 1214 },
  horizon_scattering: { x: 2862, y: 1061 },
  sun_direct: { x: 3264, y: 1651 },
  moon_direct: { x: 3264, y: 1440 },
  clear_sky_indirect: { x: 3264, y: 1229 },
  canopy_indirect: { x: 3264, y: 993 },
  light_mix: { x: 3666, y: 1429 },
  low_cloud_shadow: { x: 3666, y: 849 },
  canopy_shadow: { x: 3666, y: 1665 },
  canopy_on_low: { x: 3666, y: 1868 },
  surface_climate: { x: 2058, y: 345 },
  surface_materials: { x: 2460, y: 268 },
  surface_texture: { x: 3666, y: 336 },
  vegetation: { x: 2862, y: 370 },
  seasonal_snow: { x: 2862, y: 192 },
  ocean_forcing: { x: 2460, y: 584 },
  wave_spectrum: { x: 2862, y: 640 },
  decorative_water: { x: 3264, y: 692 },
  precipitation_contacts: { x: 3666, y: 638 },
  shallow_bottom: { x: 2058, y: 48 },
  landform_relief: { x: 4068, y: 489 },
  landform_shadow: { x: 3666, y: 1085 },
  surface_shading: { x: 4470, y: 1429 },
  low_cloud_body: { x: 4872, y: 1429 },
  final_color: { x: 5274, y: 1429 },
};

function estimateNodeHeight(model: ShaderGraphNode, inputCount: number): number {
  const controlsHeight = (model.controls ?? []).reduce(
    (height, control) => height + (control.type === "range" ? 54 : 34),
    0,
  );
  return 82 + Math.max(inputCount, 1) * 25 + controlsHeight + (model.mute ? 25 : 0);
}

async function automaticLayout(): Promise<Map<string, { x: number; y: number }>> {
  const { default: ELK } = await import("elkjs/lib/elk.bundled.js");
  const elk = new ELK();
  const inputCounts = new Map(shaderGraphNodes.map((node) => [node.id, 0]));
  for (const edge of shaderGraphEdges) {
    inputCounts.set(edge.target, inputCounts.get(edge.target)! + 1);
  }
  const graph = await elk.layout({
    id: "fragment-material",
    layoutOptions: {
      "elk.algorithm": "layered",
      "elk.direction": "RIGHT",
      "elk.edgeRouting": "ORTHOGONAL",
      "elk.spacing.nodeNode": "54",
      "elk.layered.spacing.nodeNodeBetweenLayers": "142",
      "elk.layered.nodePlacement.strategy": "NETWORK_SIMPLEX",
      "elk.layered.crossingMinimization.strategy": "LAYER_SWEEP",
      "elk.padding": "[top=48,left=48,bottom=48,right=48]",
    },
    children: shaderGraphNodes.map((node) => ({
      id: node.id,
      width: nodeWidth,
      height: estimateNodeHeight(node, inputCounts.get(node.id)!),
    })),
    edges: shaderGraphEdges.map((edge) => ({
      id: edge.id,
      sources: [edge.source],
      targets: [edge.target],
    })),
  });
  const positions = new Map((graph.children ?? []).map((node) => [
    node.id,
    { x: node.x ?? 0, y: node.y ?? 0 },
  ]));
  const compositionY = positions.get("light_mix")?.y ?? 0;
  for (const id of ["surface_shading", "low_cloud_body", "final_color"]) {
    const position = positions.get(id);
    if (position) positions.set(id, { ...position, y: compositionY });
  }
  return positions;
}

function numericValue(value: ShaderGraphValue): number {
  return typeof value === "number" ? value : Number(value);
}

function formatControlValue(control: ShaderGraphControl, value: ShaderGraphValue): string {
  if (control.type === "text") return String(value);
  const number = numericValue(value);
  if (control.type === "toggle") return number > 0.5 ? "On" : "Off";
  const displayed = number * (control.displayScale ?? 1);
  const scaledStep = control.step * (control.displayScale ?? 1);
  const formatted = displayed.toFixed(scaledStep < 0.01 ? 3 : scaledStep < 1 ? 2 : 0);
  return `${formatted}${control.suffix ?? ""}`;
}

function InlineControl({
  control,
  bindings,
  onControl,
}: {
  control: ShaderGraphControl;
  bindings: ShaderGraphBindings;
  onControl: (key: string, value: ShaderGraphValue) => void;
}) {
  const value = bindings.getValue(control.key);
  const defaultValue = bindings.getDefaultValue(control.key);
  if (control.type === "text") {
    return (
      <label className="shader-inline-text nodrag nopan">
        <span>{control.label}</span>
        <input
          className="nowheel"
          type="text"
          value={String(value)}
          onChange={(event) => onControl(control.key, event.target.value)}
        />
        <button
          type="button"
          hidden={value === defaultValue}
          onClick={() => onControl(control.key, defaultValue)}
          aria-label={`Reset ${control.label}`}
          title={`Reset ${control.label}`}
        >↺</button>
      </label>
    );
  }
  if (control.type === "toggle") {
    return (
      <label className="shader-inline-toggle nodrag nopan">
        <input
          type="checkbox"
          checked={numericValue(value) > 0.5}
          onChange={(event) => onControl(control.key, Number(event.target.checked))}
        />
        <span>{control.label}</span>
        <output>{formatControlValue(control, value)}</output>
      </label>
    );
  }
  return (
    <div className="shader-inline-range nodrag nopan">
      <label htmlFor={`shader-control-${control.key}`}>{control.label}</label>
      <output>{formatControlValue(control, value)}</output>
      <input
        className="nowheel"
        id={`shader-control-${control.key}`}
        type="range"
        min={control.min}
        max={control.max}
        step={control.step}
        value={numericValue(value)}
        onChange={(event) => onControl(control.key, Number(event.target.value))}
      />
      <button
        type="button"
        hidden={value === defaultValue}
        onClick={() => onControl(control.key, defaultValue)}
        aria-label={`Reset ${control.label}`}
        title={`Reset ${control.label}`}
      >↺</button>
    </div>
  );
}

function ShaderNodeView({ data }: NodeProps<ShaderFlowNode>) {
  const { model, inputs, muted, bindings, onControl, onMute } = data;
  return (
    <article className={`shader-node kind-${model.kind} domain-${model.domain}${muted ? " muted" : ""}`}>
      <header>
        <span className="shader-node-kind">{kindLabels[model.kind]}</span>
        <strong>{model.label}</strong>
        {model.mute ? (
          <button
            type="button"
            className="shader-node-mute nodrag nopan"
            aria-label={`${muted ? "Enable" : "Mute"} ${model.label}`}
            title={muted ? `Muted: ${model.mute.label}` : `Mute: ${model.mute.label}`}
            onClick={(event) => {
              event.stopPropagation();
              onMute(model, muted);
            }}
          >
            {muted ? "○" : "●"}
          </button>
        ) : null}
      </header>
      <div className="shader-node-inputs">
        {inputs.length === 0 ? <span className="shader-node-empty">host supplied</span> : inputs.map((input) => (
          <div className="shader-node-socket input" key={input.id}>
            <Handle
              id={input.id}
              type="target"
              position={Position.Left}
              style={{ background: socketColors[input.type] }}
            />
            <span>{input.input}</span>
          </div>
        ))}
      </div>
      {model.controls?.length ? (
        <div className="shader-node-controls">
          {model.controls.map((control) => (
            <InlineControl
              key={control.key}
              control={control}
              bindings={bindings}
              onControl={onControl}
            />
          ))}
        </div>
      ) : null}
      {muted && model.mute ? (
        <div className={`shader-node-replacement replacement-${model.mute.replacement}`}>
          Muted → {model.mute.label}
        </div>
      ) : null}
      <footer>
        <span>{model.output}</span>
        <Handle
          id="output"
          type="source"
          position={Position.Right}
          style={{ background: socketColors[model.outputType] }}
        />
      </footer>
    </article>
  );
}

const nodeTypes = { shader: ShaderNodeView };

function NodeInspector({
  node,
  bindings,
  revision,
  refresh,
}: {
  node: ShaderGraphNode;
  bindings: ShaderGraphBindings;
  revision: number;
  refresh: () => void;
}) {
  void revision;
  return (
    <Panel position="top-right" className="shader-inspector">
      <div className="shader-inspector-heading">
        <span className={`shader-kind-badge kind-${node.kind}`}>{kindLabels[node.kind]}</span>
        <strong>{node.label}</strong>
      </div>
      <p>{node.summary}</p>
      {node.mute ? (
        <div className={`shader-mute-contract replacement-${node.mute.replacement}`}>
          <strong>Muted output</strong>
          <span>{node.mute.label}</span>
          <small>Consumers continue with this neutral or fallback value.</small>
        </div>
      ) : (
        <div className="shader-readonly">This node always participates in the graph.</div>
      )}
      {node.diagnostics?.length ? (
        <div className="shader-diagnostics">
          <span>Preview output</span>
          {node.diagnostics.map((diagnostic) => (
            <button
              type="button"
              className={bindings.getDiagnostic() === diagnostic.id ? "active" : ""}
              key={diagnostic.id}
              onClick={() => {
                bindings.setDiagnostic(diagnostic.id);
                refresh();
              }}
            >
              {diagnostic.label}
            </button>
          ))}
        </div>
      ) : null}
    </Panel>
  );
}

type MaterialGraphHandle = { sync: () => void };

const MaterialGraph = forwardRef<MaterialGraphHandle, { bindings: ShaderGraphBindings }>(
  function MaterialGraph({ bindings }, ref) {
    const [revision, setRevision] = useState(0);
    const [selectedNodeId, setSelectedNodeId] = useState("final_color");
    const [instance, setInstance] = useState<ReactFlowInstance<ShaderFlowNode, Edge> | null>(null);
    const refresh = useCallback(() => setRevision((value) => value + 1), []);
    useImperativeHandle(ref, () => ({ sync: refresh }));

    const inputsByNode = useMemo(() => {
      const inputs = new Map(shaderGraphNodes.map((node) => [node.id, [] as ShaderGraphEdge[]]));
      for (const edge of shaderGraphEdges) inputs.get(edge.target)!.push(edge);
      return inputs;
    }, []);
    const dataFor = useCallback((model: ShaderGraphNode): ShaderNodeData => ({
      model,
      inputs: inputsByNode.get(model.id)!,
      muted: model.mute ? numericValue(bindings.getValue(model.mute.key)) <= 0.5 : false,
      bindings,
      onControl: (key, value) => {
        bindings.setValue(key, value);
        refresh();
      },
      onMute: (target, muted) => {
        bindings.setValue(target.mute!.key, Number(muted));
        refresh();
      },
    }), [bindings, inputsByNode, refresh]);
    const initialNodes = useMemo<ShaderFlowNode[]>(() => shaderGraphNodes.map((model, index) => ({
      id: model.id,
      type: "shader",
      position: initialPositions[model.id] ?? { x: 0, y: 260 * index },
      selectable: true,
      data: dataFor(model),
    })), [dataFor]);
    const [flowNodes, setFlowNodes, onNodesChange] = useNodesState<ShaderFlowNode>(initialNodes);
    useEffect(() => {
      setFlowNodes((nodes) => nodes.map((node) => ({
        ...node,
        data: dataFor(node.data.model),
      })));
    }, [dataFor, revision, setFlowNodes]);

    const arrange = useCallback(async () => {
      const positions = await automaticLayout();
      setFlowNodes((nodes) => nodes.map((node) => ({
        ...node,
        position: positions.get(node.id) ?? node.position,
      })));
      requestAnimationFrame(() => requestAnimationFrame(() => {
        if (!instance) return;
        instance.fitView({ nodes: instance.getNodes(), padding: 0.08, duration: 350 });
      }));
    }, [instance, setFlowNodes]);
    const flowEdges = useMemo<Edge[]>(() => shaderGraphEdges.map((edge) => ({
      id: edge.id,
      source: edge.source,
      sourceHandle: "output",
      target: edge.target,
      targetHandle: edge.id,
      type: "smoothstep",
      className: `shader-edge socket-${edge.type}`,
      markerEnd: { type: MarkerType.ArrowClosed, color: socketColors[edge.type], width: 10, height: 10 },
      style: { stroke: socketColors[edge.type] },
    })), []);
    const selectedNode = shaderGraphNodes.find((node) => node.id === selectedNodeId)!;

    return (
      <ReactFlow<ShaderFlowNode, Edge>
        nodes={flowNodes}
        edges={flowEdges}
        nodeTypes={nodeTypes}
        onNodesChange={onNodesChange}
        nodesDraggable
        nodesConnectable={false}
        edgesReconnectable={false}
        elementsSelectable
        panOnDrag
        zoomOnScroll
        zoomOnPinch
        minZoom={0.1}
        maxZoom={1.8}
        defaultViewport={{ x: -2400, y: -450, zoom: 0.55 }}
        onInit={setInstance}
        onNodeClick={(_, node) => setSelectedNodeId(node.id)}
        colorMode="dark"
        deleteKeyCode={null}
      >
        <Background color="#34505a" gap={22} size={1} />
        <Controls showInteractive={false} position="bottom-left" />
        <MiniMap
          pannable
          zoomable
          position="bottom-right"
          nodeColor={(node) => {
            const model = (node.data as ShaderNodeData).model;
            return model.kind === "output" ? "#72c99c" : model.kind === "physical" ? "#67bed8" : "#526c73";
          }}
          maskColor="rgb(4 16 21 / 78%)"
        />
        <Panel position="top-left" className="shader-graph-key">
          <strong>Fragment material</strong>
          <span>Drag nodes or background · wheel to zoom · select to inspect</span>
          <div>
            <button type="button" onClick={() => { void arrange(); }}>Auto layout</button>
            <button
              type="button"
              onClick={() => {
                for (const model of shaderGraphNodes) {
                  if (model.kind === "input") continue;
                  for (const control of model.controls ?? []) {
                    bindings.setValue(control.key, bindings.getDefaultValue(control.key));
                  }
                }
                refresh();
              }}
            >
              Production tuning
            </button>
          </div>
        </Panel>
        <NodeInspector
          node={selectedNode}
          bindings={bindings}
          revision={revision}
          refresh={refresh}
        />
      </ReactFlow>
    );
  },
);

export function mountShaderGraph(
  element: HTMLElement,
  bindings: ShaderGraphBindings,
): ShaderGraphController {
  validateShaderGraph();
  const ref = createRef<MaterialGraphHandle>();
  const root = createRoot(element);
  root.render(<MaterialGraph bindings={bindings} ref={ref} />);
  return {
    sync: () => ref.current?.sync(),
    destroy: () => root.unmount(),
  };
}
