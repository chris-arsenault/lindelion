import { useEffect, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import "./styles.css";

interface ReviewCase {
  id: string;
  title: string;
  groupId: string;
  wav: string;
  audioPath: string;
  audioUrl: string;
  audioFormat: "mp3" | "wav";
  tags: string[];
  durationSeconds: number;
  peakDbfs: number;
  rmsDbfs: number;
  fileBytes: number;
}

interface ReviewGroup {
  id: string;
  directory: string;
  title: string;
  question: string;
  cases: ReviewCase[];
}

interface ReviewCategory {
  id: string;
  type: "group" | "tag";
  value: string;
  label: string;
  count: number;
  caseIds: string[];
}

interface CatalogResponse {
  groups: ReviewGroup[];
  categories: ReviewCategory[];
  commentsPath: string;
}

interface CommentEntry {
  caseId: string;
  comment: string;
  updatedAt: string;
}

interface CategoryCommentEntry {
  categoryId: string;
  categoryType: "group" | "tag" | "category";
  categoryValue: string;
  comment: string;
  updatedAt: string;
}

interface CommentsResponse {
  comments: Record<string, CommentEntry>;
  categoryComments?: Record<string, CategoryCommentEntry>;
}

type SaveState = "idle" | "dirty" | "saving" | "saved" | "error";

function App() {
  const [catalog, setCatalog] = useState<CatalogResponse | null>(null);
  const [comments, setComments] = useState<Record<string, string>>({});
  const [savedComments, setSavedComments] = useState<Record<string, string>>({});
  const [saveState, setSaveState] = useState<Record<string, SaveState>>({});
  const [categoryComments, setCategoryComments] = useState<Record<string, string>>({});
  const [savedCategoryComments, setSavedCategoryComments] = useState<Record<string, string>>({});
  const [categorySaveState, setCategorySaveState] = useState<Record<string, SaveState>>({});
  const [activeGroupId, setActiveGroupId] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [loadError, setLoadError] = useState<string | null>(null);

  useEffect(() => {
    async function load() {
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
        const loadedCategoryComments = Object.fromEntries(
          Object.entries(commentsResponse.categoryComments ?? {}).map(([categoryId, entry]) => [
            categoryId,
            entry.comment
          ])
        );
        setCatalog(catalogResponse);
        setActiveGroupId(catalogResponse.groups[0]?.id ?? null);
        setComments(loadedComments);
        setSavedComments(loadedComments);
        setCategoryComments(loadedCategoryComments);
        setSavedCategoryComments(loadedCategoryComments);
      } catch (error) {
        setLoadError(error instanceof Error ? error.message : String(error));
      }
    }
    void load();
  }, []);

  const visibleGroups = useMemo(() => {
    if (!catalog) {
      return [];
    }
    const needle = query.trim().toLowerCase();
    return catalog.groups
      .filter((group) => !activeGroupId || group.id === activeGroupId)
      .map((group) => ({
        ...group,
        cases:
          needle.length === 0
            ? group.cases
            : group.cases.filter((item) => caseMatches(item, needle))
      }))
      .filter((group) => group.cases.length > 0 || query.trim().length === 0);
  }, [activeGroupId, catalog, query]);

  const visibleCategories = useMemo(() => {
    if (!catalog) {
      return [];
    }
    const needle = query.trim().toLowerCase();
    return needle.length === 0
      ? catalog.categories
      : catalog.categories.filter((category) => categoryMatches(category, needle));
  }, [catalog, query]);

  if (loadError) {
    return <main className="load-error">{loadError}</main>;
  }

  if (!catalog) {
    return <main className="loading">Loading catalog</main>;
  }

  const totalCases = catalog.groups.reduce((sum, group) => sum + group.cases.length, 0);
  const commentedCount = Object.values(savedComments).filter(
    (comment) => comment.trim().length > 0
  ).length;
  const categoryCommentedCount = Object.values(savedCategoryComments).filter(
    (comment) => comment.trim().length > 0
  ).length;

  async function saveOnBlur(caseId: string) {
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

  async function saveCategoryOnBlur(categoryId: string) {
    const next = categoryComments[categoryId] ?? "";
    if (next === (savedCategoryComments[categoryId] ?? "")) {
      setCategorySaveState((state) => ({ ...state, [categoryId]: "idle" }));
      return;
    }
    setCategorySaveState((state) => ({ ...state, [categoryId]: "saving" }));
    try {
      await fetchJson<CategoryCommentEntry>(
        `/api/category-comments/${encodeURIComponent(categoryId)}`,
        {
          method: "PUT",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ comment: next })
        }
      );
      setSavedCategoryComments((state) => ({ ...state, [categoryId]: next }));
      setCategorySaveState((state) => ({ ...state, [categoryId]: "saved" }));
    } catch {
      setCategorySaveState((state) => ({ ...state, [categoryId]: "error" }));
    }
  }

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="app-title">
          <h1>Lamath Review</h1>
          <p>{totalCases} files</p>
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
              <span className="folder-count">{group.cases.length}</span>
            </button>
          ))}
        </nav>
      </aside>

      <main className="review-pane">
        <header className="toolbar">
          <div>
            <h2>{catalog.groups.find((group) => group.id === activeGroupId)?.title}</h2>
            <p>
              {commentedCount} file comments / {categoryCommentedCount} category comments
            </p>
          </div>
          <input
            aria-label="Filter cases"
            className="filter-input"
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Filter"
            value={query}
          />
        </header>

        <details className="category-feedback" open>
          <summary>
            <span>Category Feedback</span>
            <span>
              {visibleCategories.length} shown / {catalog.categories.length} total
            </span>
          </summary>
          {visibleCategories.length === 0 ? (
            <p className="empty-note">No matching categories</p>
          ) : (
            <div className="category-comment-grid">
              {visibleCategories.map((category) => (
                <article className="category-comment-row" key={category.id}>
                  <div className="category-meta">
                    <div className="category-title-line">
                      <span className="category-type">{category.type}</span>
                      <strong>{category.label}</strong>
                    </div>
                    <div className="category-detail">
                      {category.id} / {category.count} files
                    </div>
                  </div>
                  <div className="category-comment-cell">
                    <textarea
                      aria-label={`Category comment for ${category.label}`}
                      onBlur={() => void saveCategoryOnBlur(category.id)}
                      onChange={(event) => {
                        const comment = event.target.value;
                        setCategoryComments((state) => ({
                          ...state,
                          [category.id]: comment
                        }));
                        setCategorySaveState((state) => ({
                          ...state,
                          [category.id]: "dirty"
                        }));
                      }}
                      placeholder="Category comment"
                      value={categoryComments[category.id] ?? ""}
                    />
                    <div className={`save-state ${categorySaveState[category.id] ?? "idle"}`}>
                      {statusLabel(categorySaveState[category.id])}
                    </div>
                  </div>
                </article>
              ))}
            </div>
          )}
        </details>

        <div className="case-groups">
          {visibleGroups.map((group) => (
            <section className="case-group" key={group.id}>
              <div className="group-heading">
                <div>
                  <h3>{group.directory}</h3>
                  <p>{group.question}</p>
                </div>
              </div>
              {group.cases.map((item) => (
                <article className="case-row" key={item.id}>
                  <div className="case-main">
                    <div className="case-title-line">
                      <h4>{item.title}</h4>
                      <span className="format-badge">{item.audioFormat}</span>
                    </div>
                    <div className="case-path">{item.audioPath}</div>
                    <div className="tag-row">
                      {item.tags.map((tag) => (
                        <span className="tag" key={tag}>
                          {tag}
                        </span>
                      ))}
                    </div>
                    <audio className="audio-player" controls preload="metadata" src={item.audioUrl} />
                  </div>
                  <div className="metrics">
                    <Metric label="Peak" value={`${item.peakDbfs.toFixed(2)} dBFS`} />
                    <Metric label="RMS" value={`${item.rmsDbfs.toFixed(2)} dBFS`} />
                    <Metric label="Time" value={`${item.durationSeconds.toFixed(2)}s`} />
                  </div>
                  <div className="comment-cell">
                    <textarea
                      aria-label={`Comment for ${item.title}`}
                      onBlur={() => void saveOnBlur(item.id)}
                      onChange={(event) => {
                        const comment = event.target.value;
                        setComments((state) => ({ ...state, [item.id]: comment }));
                        setSaveState((state) => ({ ...state, [item.id]: "dirty" }));
                      }}
                      placeholder="Comment"
                      value={comments[item.id] ?? ""}
                    />
                    <div className={`save-state ${saveState[item.id] ?? "idle"}`}>
                      {statusLabel(saveState[item.id])}
                    </div>
                  </div>
                </article>
              ))}
            </section>
          ))}
        </div>
      </main>
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="metric">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function caseMatches(item: ReviewCase, needle: string): boolean {
  return [
    item.id,
    item.title,
    item.wav,
    item.audioPath,
    item.groupId,
    ...item.tags
  ].some((value) => value.toLowerCase().includes(needle));
}

function categoryMatches(category: ReviewCategory, needle: string): boolean {
  return [
    category.id,
    category.type,
    category.value,
    category.label,
    String(category.count),
    ...category.caseIds
  ].some((value) => value.toLowerCase().includes(needle));
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
