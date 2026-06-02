import express from "express";
import { createReadStream } from "node:fs";
import { mkdir, readFile, rename, stat, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parse } from "smol-toml";
import { createServer as createViteServer } from "vite";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const appRoot = path.resolve(__dirname, "..");
const repoRoot = path.resolve(appRoot, "../..");

const manifestPath = envPath(
  "LAMATH_REVIEW_MANIFEST",
  "review/lamath-render-catalog/manifest.toml"
);
const wavRoot = envPath("LAMATH_REVIEW_WAV_ROOT", "review/lamath-render-catalog");
const previewRoot = envPath(
  "LAMATH_REVIEW_PREVIEW_ROOT",
  "review/audio-previews/lamath-render-catalog"
);
const commentsPath = envPath(
  "LAMATH_REVIEW_COMMENTS_FILE",
  "review/lamath-render-catalog-comments.json"
);

const host = process.env.HOST ?? "0.0.0.0";
const port = Number(process.env.PORT ?? 26000);
const publicHost = process.env.SULION_PUBLIC_HOST ?? "192.168.66.3";

interface ManifestGroup {
  id: string;
  directory: string;
  title: string;
  question: string;
}

interface ManifestCase {
  id: string;
  title: string;
  group_id: string;
  wav: string;
  tags: string[];
  duration_seconds: number;
  peak_dbfs: number;
  rms_dbfs: number;
  file_bytes: number;
}

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

interface CaseCommentEntry {
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

interface CommentsFile {
  schemaVersion: 2;
  updatedAt: string;
  comments: Record<string, CaseCommentEntry>;
  categoryComments: Record<string, CategoryCommentEntry>;
}

async function main() {
  const app = express();
  app.use(express.json({ limit: "128kb" }));

  app.get("/api/catalog", async (_request, response, next) => {
    try {
      response.json(await loadCatalog());
    } catch (error) {
      next(error);
    }
  });

  app.get("/api/comments", async (_request, response, next) => {
    try {
      response.json(await readComments());
    } catch (error) {
      next(error);
    }
  });

  app.put("/api/comments/:caseId", async (request, response, next) => {
    try {
      const comment = request.body?.comment;
      if (typeof comment !== "string") {
        response.status(400).json({ error: "comment must be a string" });
        return;
      }
      response.json(await saveComment(request.params.caseId, comment));
    } catch (error) {
      next(error);
    }
  });

  app.put("/api/category-comments/:categoryId", async (request, response, next) => {
    try {
      const comment = request.body?.comment;
      if (typeof comment !== "string") {
        response.status(400).json({ error: "comment must be a string" });
        return;
      }
      response.json(await saveCategoryComment(request.params.categoryId, comment));
    } catch (error) {
      next(error);
    }
  });

  app.get("/media/:format/*", async (request, response, next) => {
    try {
      const format = request.params.format;
      if (format !== "preview" && format !== "wav") {
        response.status(404).end();
        return;
      }
      const relativePath = (request.params as { 0?: string })[0] ?? "";
      const root = format === "preview" ? previewRoot : wavRoot;
      const filePath = safeJoin(root, relativePath);
      await stat(filePath);
      response.type(format === "preview" ? "audio/mpeg" : "audio/wav");
      createReadStream(filePath).pipe(response);
    } catch (error) {
      next(error);
    }
  });

  const vite = await createViteServer({
    root: appRoot,
    server: { middlewareMode: true },
    appType: "spa"
  });
  app.use(vite.middlewares);

  app.use((error: unknown, _request: express.Request, response: express.Response) => {
    const message = error instanceof Error ? error.message : String(error);
    response.status(500).json({ error: message });
  });

  app.listen(port, host, () => {
    console.log(`Lamath review UI: http://${publicHost}:${port}`);
    console.log(`Listening: http://${host}:${port}`);
    console.log(`Manifest: ${manifestPath}`);
    console.log(`Comments: ${commentsPath}`);
  });
}

function envPath(name: string, fallback: string): string {
  return path.resolve(repoRoot, process.env[name] ?? fallback);
}

async function loadCatalog(): Promise<{
  groups: ReviewGroup[];
  categories: ReviewCategory[];
  commentsPath: string;
}> {
  const manifest = parse(await readFile(manifestPath, "utf8")) as unknown as {
    groups?: ManifestGroup[];
    cases?: ManifestCase[];
  };
  const groups = manifest.groups ?? [];
  const cases = manifest.cases ?? [];
  const casesByGroup = new Map<string, ReviewCase[]>();

  for (const item of cases) {
    const audio = await audioForCase(item.wav);
    const reviewCase: ReviewCase = {
      id: item.id,
      title: item.title,
      groupId: item.group_id,
      wav: item.wav,
      audioPath: audio.relativePath,
      audioUrl: mediaUrl(audio.format, audio.relativePath),
      audioFormat: audio.format,
      tags: item.tags,
      durationSeconds: item.duration_seconds,
      peakDbfs: item.peak_dbfs,
      rmsDbfs: item.rms_dbfs,
      fileBytes: item.file_bytes
    };
    const groupCases = casesByGroup.get(item.group_id) ?? [];
    groupCases.push(reviewCase);
    casesByGroup.set(item.group_id, groupCases);
  }

  const reviewGroups = groups.map((group) => ({
      id: group.id,
      directory: group.directory,
      title: group.title,
      question: group.question,
      cases: casesByGroup.get(group.id) ?? []
    }));

  return {
    groups: reviewGroups,
    categories: buildCategories(reviewGroups),
    commentsPath
  };
}

function buildCategories(groups: ReviewGroup[]): ReviewCategory[] {
  const categories: ReviewCategory[] = groups
    .filter((group) => group.cases.length > 0)
    .map((group) => ({
      id: `group:${group.id}`,
      type: "group",
      value: group.id,
      label: group.title,
      count: group.cases.length,
      caseIds: group.cases.map((item) => item.id)
    }));

  const caseIdsByTag = new Map<string, string[]>();
  for (const group of groups) {
    for (const item of group.cases) {
      for (const tag of item.tags) {
        const caseIds = caseIdsByTag.get(tag) ?? [];
        caseIds.push(item.id);
        caseIdsByTag.set(tag, caseIds);
      }
    }
  }

  const tagCategories = Array.from(caseIdsByTag.entries())
    .filter(([, caseIds]) => caseIds.length > 1)
    .sort(([leftTag, leftCases], [rightTag, rightCases]) => {
      const countSort = rightCases.length - leftCases.length;
      return countSort !== 0 ? countSort : leftTag.localeCompare(rightTag);
    })
    .map(([tag, caseIds]) => ({
      id: `tag:${tag}`,
      type: "tag" as const,
      value: tag,
      label: tag,
      count: caseIds.length,
      caseIds
    }));

  return [...categories, ...tagCategories];
}

async function audioForCase(wav: string): Promise<{ format: "mp3" | "wav"; relativePath: string }> {
  const preview = wav.replace(/\.wav$/i, ".mp3");
  try {
    await stat(safeJoin(previewRoot, preview));
    return { format: "mp3", relativePath: preview };
  } catch {
    return { format: "wav", relativePath: wav };
  }
}

function mediaUrl(format: "mp3" | "wav", relativePath: string): string {
  const prefix = format === "mp3" ? "preview" : "wav";
  return `/media/${prefix}/${relativePath
    .split("/")
    .map((segment) => encodeURIComponent(segment))
    .join("/")}`;
}

async function readComments(): Promise<CommentsFile> {
  try {
    const parsed = JSON.parse(await readFile(commentsPath, "utf8")) as CommentsFile;
    return {
      schemaVersion: 2,
      updatedAt: parsed.updatedAt ?? new Date(0).toISOString(),
      comments: parsed.comments ?? {},
      categoryComments: parsed.categoryComments ?? {}
    };
  } catch (error) {
    if (error instanceof Error && "code" in error && error.code === "ENOENT") {
      return emptyComments();
    }
    throw error;
  }
}

async function saveComment(caseId: string, comment: string): Promise<CaseCommentEntry> {
  const comments = await readComments();
  const entry = {
    caseId,
    comment,
    updatedAt: new Date().toISOString()
  };
  comments.updatedAt = entry.updatedAt;
  if (comment.trim().length === 0) {
    delete comments.comments[caseId];
  } else {
    comments.comments[caseId] = entry;
  }
  await writeComments(comments);
  return entry;
}

async function saveCategoryComment(
  categoryId: string,
  comment: string
): Promise<CategoryCommentEntry> {
  const comments = await readComments();
  const category = parseCategoryId(categoryId);
  const entry = {
    categoryId,
    categoryType: category.type,
    categoryValue: category.value,
    comment,
    updatedAt: new Date().toISOString()
  };
  comments.updatedAt = entry.updatedAt;
  if (comment.trim().length === 0) {
    delete comments.categoryComments[categoryId];
  } else {
    comments.categoryComments[categoryId] = entry;
  }
  await writeComments(comments);
  return entry;
}

function parseCategoryId(categoryId: string): {
  type: CategoryCommentEntry["categoryType"];
  value: string;
} {
  const separator = categoryId.indexOf(":");
  if (separator === -1) {
    return { type: "category", value: categoryId };
  }
  const type = categoryId.slice(0, separator);
  const value = categoryId.slice(separator + 1);
  if (type === "group" || type === "tag") {
    return { type, value };
  }
  return { type: "category", value };
}

async function writeComments(comments: CommentsFile): Promise<void> {
  await mkdir(path.dirname(commentsPath), { recursive: true });
  const tempPath = `${commentsPath}.tmp`;
  await writeFile(tempPath, `${JSON.stringify(comments, null, 2)}\n`);
  await rename(tempPath, commentsPath);
}

function emptyComments(): CommentsFile {
  return {
    schemaVersion: 2,
    updatedAt: new Date(0).toISOString(),
    comments: {},
    categoryComments: {}
  };
}

function safeJoin(root: string, relativePath: string): string {
  if (relativePath.includes("\0")) {
    throw new Error("invalid media path");
  }
  const resolved = path.resolve(root, relativePath);
  const normalizedRoot = path.resolve(root);
  if (resolved !== normalizedRoot && !resolved.startsWith(`${normalizedRoot}${path.sep}`)) {
    throw new Error(`media path escapes root: ${relativePath}`);
  }
  return resolved;
}

main().catch((error: unknown) => {
  console.error(error);
  process.exitCode = 1;
});
