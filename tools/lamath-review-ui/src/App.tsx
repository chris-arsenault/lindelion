import { useCallback, useEffect, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import "./styles.css";

interface ReviewCase {
  id: string;
  title: string;
  groupId: string;
  wav: string;
  audioPath: string;
  audioUrl: string;
  audioFormat: "wav";
  tags: string[];
  durationSeconds: number;
  peakDbfs: number;
  rmsDbfs: number;
  fileBytes: number;
  createdAt: string | null;
}

interface AxisOption {
  value: string;
  label: string;
}

interface VariantAxis {
  id: string;
  label: string;
  varying: boolean;
  options: AxisOption[];
}

interface Variant {
  coords: Record<string, string>;
  case: ReviewCase;
}

interface VariantFamily {
  id: string;
  axes: VariantAxis[];
  variants: Variant[];
}

interface ReviewGroup {
  id: string;
  directory: string;
  title: string;
  question: string;
  families: VariantFamily[];
}

interface CatalogResponse {
  groups: ReviewGroup[];
  commentsPath: string;
}

interface CommentEntry {
  caseId: string;
  comment: string;
  updatedAt: string;
}

interface CommentsResponse {
  comments: Record<string, CommentEntry>;
}

type SaveState = "idle" | "dirty" | "saving" | "saved" | "error";

interface CommentController {
  comments: Record<string, string>;
  saveState: Record<string, SaveState>;
  edit: (caseId: string, comment: string) => void;
  save: (caseId: string) => void;
}

function App() {
  const [catalog, setCatalog] = useState<CatalogResponse | null>(null);
  const [comments, setComments] = useState<Record<string, string>>({});
  const [savedComments, setSavedComments] = useState<Record<string, string>>({});
  const [saveState, setSaveState] = useState<Record<string, SaveState>>({});
  const [activeGroupId, setActiveGroupId] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [loadError, setLoadError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);

  const load = useCallback(async () => {
    setRefreshing(true);
    try {
      const [catalogResponse, commentsResponse] = await Promise.all([
        fetchJson<CatalogResponse>("/api/catalog"),
        fetchJson<CommentsResponse>("/api/comments")
      ]);
      const loadedComments = Object.fromEntries(
        Object.entries(commentsResponse.comments).map(([caseId, entry]) => [
          caseId,
          entry.comment
        ])
      );
      setCatalog(catalogResponse);
      setActiveGroupId((current) => {
        if (current && catalogResponse.groups.some((group) => group.id === current)) {
          return current;
        }
        return catalogResponse.groups[0]?.id ?? null;
      });
      setComments(loadedComments);
      setSavedComments(loadedComments);
      setLoadError(null);
    } catch (error) {
      setLoadError(error instanceof Error ? error.message : String(error));
    } finally {
      setRefreshing(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const visibleGroups = useMemo(() => {
    if (!catalog) {
      return [];
    }
    const needle = query.trim().toLowerCase();
    return catalog.groups
      .filter((group) => !activeGroupId || group.id === activeGroupId)
      .map((group) => ({
        ...group,
        families:
          needle.length === 0
            ? group.families
            : group.families.filter((family) => familyMatches(family, needle))
      }))
      .filter((group) => group.families.length > 0 || query.trim().length === 0);
  }, [activeGroupId, catalog, query]);

  if (loadError) {
    return <main className="load-error">{loadError}</main>;
  }

  if (!catalog) {
    return <main className="loading">Loading catalog</main>;
  }

  const totalFiles = catalog.groups.reduce(
    (sum, group) => sum + group.families.reduce((count, family) => count + family.variants.length, 0),
    0
  );
  const commentedCount = Object.values(savedComments).filter(
    (comment) => comment.trim().length > 0
  ).length;

  const edit = (caseId: string, comment: string) => {
    setComments((state) => ({ ...state, [caseId]: comment }));
    setSaveState((state) => ({ ...state, [caseId]: "dirty" }));
  };

  async function save(caseId: string) {
    const next = comments[caseId] ?? "";
    if (next === (savedComments[caseId] ?? "")) {
      setSaveState((state) => ({ ...state, [caseId]: "idle" }));
      return;
    }
    setSaveState((state) => ({ ...state, [caseId]: "saving" }));
    try {
      await fetchJson<CommentEntry>(`/api/comments/${encodeURIComponent(caseId)}`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ comment: next })
      });
      setSavedComments((state) => ({ ...state, [caseId]: next }));
      setSaveState((state) => ({ ...state, [caseId]: "saved" }));
    } catch {
      setSaveState((state) => ({ ...state, [caseId]: "error" }));
    }
  }

  const controller: CommentController = { comments, saveState, edit, save };

  const groupFileCount = (group: ReviewGroup) =>
    group.families.reduce((count, family) => count + family.variants.length, 0);

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="app-title">
          <h1>Lamath Review</h1>
          <p>{totalFiles} files</p>
        </div>
        <nav className="group-list" aria-label="Catalog folders">
          {catalog.groups.map((group) => (
            <button
              className={group.id === activeGroupId ? "group-button active" : "group-button"}
              key={group.id}
              onClick={() => setActiveGroupId(group.id)}
              type="button"
            >
              <span className="folder-name">{group.directory}</span>
              <span className="folder-title">{group.title}</span>
              <span className="folder-count">{groupFileCount(group)}</span>
            </button>
          ))}
        </nav>
      </aside>

      <main className="review-pane">
        <header className="toolbar">
          <div>
            <h2>{catalog.groups.find((group) => group.id === activeGroupId)?.title}</h2>
            <p>{commentedCount} file comments</p>
          </div>
          <div className="toolbar-controls">
            <input
              aria-label="Filter cases"
              className="filter-input"
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Filter"
              value={query}
            />
            <button
              className="refresh-button"
              disabled={refreshing}
              onClick={() => void load()}
              type="button"
            >
              {refreshing ? "Refreshing" : "Refresh"}
            </button>
          </div>
        </header>

        <div className="case-groups">
          {visibleGroups.map((group) => (
            <section className="case-group" key={group.id}>
              <div className="group-heading">
                <div>
                  <h3>{group.directory}</h3>
                  <p>{group.question}</p>
                </div>
              </div>
              {group.families.map((family) => (
                <FamilyRow key={family.id} family={family} controller={controller} />
              ))}
            </section>
          ))}
        </div>
      </main>
    </div>
  );
}

// One variant family rendered as a single row: a selector per varying axis switches which
// rendered case the player, metrics, and comment reflect. Constant axes show as fixed context.
function FamilyRow({
  family,
  controller
}: {
  family: VariantFamily;
  controller: CommentController;
}) {
  const [coords, setCoords] = useState<Record<string, string>>(
    () => family.variants[0]?.coords ?? {}
  );
  const selected = useMemo(() => resolveVariant(family, coords), [family, coords]);
  const item = selected.case;
  const audioSrc = cacheBustedAudioUrl(item);
  const selectors = family.axes.filter(
    (axis) => axis.varying && !(axis.id === "variant" && axis.options.length <= 1)
  );
  const constants = family.axes.filter(
    (axis) => !axis.varying && axis.id !== "variant" && (axis.options[0]?.label ?? "") !== ""
  );

  const pick = (axisId: string, value: string) => {
    setCoords((current) => snapCoords(family, current, axisId, value));
  };

  return (
    <article className="case-row">
      <div className="case-main">
        <div className="case-title-line">
          <h4>{item.title}</h4>
          <span className="format-badge">{item.audioFormat}</span>
        </div>
        <div className="case-path">{item.audioPath}</div>
        {(selectors.length > 0 || constants.length > 0) && (
          <div className="axis-bar">
            {selectors.map((axis) => (
              <div className="axis-group" key={axis.id}>
                <span className="axis-label">{axis.label}</span>
                <div className="axis-options">
                  {axis.options.map((option) => (
                    <button
                      className={
                        coords[axis.id] === option.value
                          ? "axis-option active"
                          : "axis-option"
                      }
                      key={option.value}
                      onClick={() => pick(axis.id, option.value)}
                      type="button"
                    >
                      {option.label}
                    </button>
                  ))}
                </div>
              </div>
            ))}
            {constants.map((axis) => (
              <span className="axis-constant" key={axis.id}>
                {axis.label}: {axis.options[0]?.label}
              </span>
            ))}
          </div>
        )}
        <div className="tag-row">
          {item.tags.map((tag) => (
            <span className="tag" key={tag}>
              {tag}
            </span>
          ))}
        </div>
        <audio
          className="audio-player"
          controls
          preload="metadata"
          src={audioSrc}
          key={audioSrc}
        />
      </div>
      <div className="metrics">
        <Metric label="Peak" value={`${item.peakDbfs.toFixed(2)} dBFS`} />
        <Metric label="RMS" value={`${item.rmsDbfs.toFixed(2)} dBFS`} />
        <Metric label="Time" value={`${item.durationSeconds.toFixed(2)}s`} />
        <Metric label="Created" value={formatCreatedAt(item.createdAt)} />
      </div>
      <div className="comment-cell">
        <textarea
          aria-label={`Comment for ${item.title}`}
          onBlur={() => void controller.save(item.id)}
          onChange={(event) => controller.edit(item.id, event.target.value)}
          placeholder="Comment"
          value={controller.comments[item.id] ?? ""}
        />
        <div className={`save-state ${controller.saveState[item.id] ?? "idle"}`}>
          {statusLabel(controller.saveState[item.id])}
        </div>
      </div>
    </article>
  );
}

// The variant whose coordinates exactly match the current selection, falling back to the first
// variant (selection is always seeded from, and snapped to, a real variant's coordinates).
function resolveVariant(family: VariantFamily, coords: Record<string, string>): Variant {
  const exact = family.variants.find((variant) =>
    Object.entries(coords).every(([axis, value]) => variant.coords[axis] === value)
  );
  return exact ?? family.variants[0];
}

// Move one axis to `value` and land on the existing variant that best preserves the other axes —
// so sparse grids (not every combination rendered) always resolve to a real, playable case.
function snapCoords(
  family: VariantFamily,
  current: Record<string, string>,
  axisId: string,
  value: string
): Record<string, string> {
  const candidates = family.variants.filter((variant) => variant.coords[axisId] === value);
  if (candidates.length === 0) {
    return current;
  }
  let best = candidates[0];
  let bestScore = -1;
  for (const candidate of candidates) {
    const score = Object.entries(current).reduce(
      (sum, [axis, axisValue]) =>
        axis !== axisId && candidate.coords[axis] === axisValue ? sum + 1 : sum,
      0
    );
    if (score > bestScore) {
      bestScore = score;
      best = candidate;
    }
  }
  return best.coords;
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="metric">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function cacheBustedAudioUrl(item: ReviewCase): string {
  const separator = item.audioUrl.includes("?") ? "&" : "?";
  const version = encodeURIComponent(`${item.createdAt ?? "missing"}-${item.fileBytes}`);
  return `${item.audioUrl}${separator}ui_v=${version}`;
}

function formatCreatedAt(createdAt: string | null): string {
  if (!createdAt) {
    return "—";
  }
  const date = new Date(createdAt);
  if (Number.isNaN(date.getTime())) {
    return "—";
  }
  return date.toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit"
  });
}

// A family matches the filter if any of its variants matches — keeps the whole switchable row
// visible when searching by family/velocity/tag.
function familyMatches(family: VariantFamily, needle: string): boolean {
  return family.variants.some((variant) => {
    const haystack = [
      variant.case.id,
      variant.case.title,
      variant.case.wav,
      variant.case.audioPath,
      variant.case.groupId,
      ...variant.case.tags,
      ...Object.values(variant.coords)
    ];
    return haystack.some((value) => value.toLowerCase().includes(needle));
  });
}

function statusLabel(state: SaveState | undefined): string {
  switch (state) {
    case "dirty":
      return "Unsaved";
    case "saving":
      return "Saving";
    case "saved":
      return "Saved";
    case "error":
      return "Save failed";
    default:
      return "";
  }
}

async function fetchJson<T>(url: string, init?: RequestInit): Promise<T> {
  const response = await fetch(url, init);
  if (!response.ok) {
    throw new Error(`${response.status} ${response.statusText}`);
  }
  return (await response.json()) as T;
}

createRoot(document.getElementById("root") as HTMLElement).render(<App />);
