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

interface ManifestAxis {
  axis: string;
  value: string;
  label: string;
}

interface ManifestCase {
  id: string;
  title: string;
  group_id: string;
  family?: string;
  wav: string;
  tags: string[];
  axes?: ManifestAxis[];
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

// One switchable dimension of a variant family. `varying` is false when every variant shares the
// same value (a fixed label, not a selector).
interface VariantAxis {
  id: string;
  label: string;
  varying: boolean;
  options: AxisOption[];
}

// One rendered case as a point in its family's grid: its coordinate per axis plus the case payload.
interface Variant {
  coords: Record<string, string>;
  case: ReviewCase;
}

// A set of cases that are the same audition differing only along `axes` — rendered as one UI row
// with one selector per varying axis. A family with a single variant is just a plain row.
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

// Display names for axis ids; falls back to a title-cased id for anything unmapped.
const AXIS_LABELS: Record<string, string> = {
  family: "Family",
  driver: "Driver",
  contact: "Contact",
  body_depth: "Body depth",
  surrounding: "Surrounding",
  edge: "Edge",
  bell: "Bell",
  body: "Body",
  reed_aperture: "Reed aperture",
  body_level: "Body level",
  radiation_shape: "Radiation",
  bore_steepening: "Bore steepening",
  articulation: "Articulation",
  gain: "Gain",
  humanize: "Humanize",
  register_key: "Register key",
  clarinet_contour: "Clarinet contour",
  output_path: "Output path",
  voicing: "Voicing",
  striker: "Striker",
  polyphony: "Polyphony",
  retrigger: "Retrigger",
  source: "Source",
  strikes: "Strikes",
  register: "Register",
  velocity: "Velocity",
  variant: "Variant"
};

function axisLabel(id: string): string {
  return AXIS_LABELS[id] ?? id.replace(/_/g, " ").replace(/^./, (char) => char.toUpperCase());
}

interface CaseCommentEntry {
  caseId: string;
  comment: string;
  updatedAt: string;
}

interface CommentsFile {
  schemaVersion: 2;
  updatedAt: string;
  comments: Record<string, CaseCommentEntry>;
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

  app.get("/media/:format/*", async (request, response, next) => {
    try {
      const format = request.params.format;
      if (format !== "wav") {
        response.status(404).end();
        return;
      }
      const relativePath = (request.params as { 0?: string })[0] ?? "";
      const filePath = safeJoin(wavRoot, relativePath);
      await stat(filePath);
      response.type("audio/wav");
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
  commentsPath: string;
}> {
  const manifest = parse(await readFile(manifestPath, "utf8")) as unknown as {
    groups?: ManifestGroup[];
    cases?: ManifestCase[];
  };
  const groups = manifest.groups ?? [];
  const cases = manifest.cases ?? [];
  const variantsByGroup = new Map<string, GroupVariant[]>();

  for (const item of cases) {
    const audio = await audioForCase(item.wav);
    const reviewCase: ReviewCase = {
      id: item.id,
      title: item.title,
      groupId: item.group_id,
      wav: item.wav,
      audioPath: audio.relativePath,
      audioUrl: mediaUrl(audio.relativePath, audio.version),
      audioFormat: audio.format,
      tags: item.tags,
      durationSeconds: item.duration_seconds,
      peakDbfs: item.peak_dbfs,
      rmsDbfs: item.rms_dbfs,
      fileBytes: item.file_bytes,
      createdAt: await renderedAt(item.wav)
    };
    // A pre-v2 manifest (no family/axes) degrades to one case per family via the case id and a
    // single-option `variant` axis, so the UI still renders it as a plain row.
    const family = item.family ?? item.id;
    const axes: ManifestAxis[] =
      item.axes && item.axes.length > 0
        ? item.axes
        : [{ axis: "variant", value: item.id, label: item.title }];
    const coords = Object.fromEntries(axes.map((axis) => [axis.axis, axis.value]));
    const groupVariants = variantsByGroup.get(item.group_id) ?? [];
    groupVariants.push({ coords, case: reviewCase, family, axes });
    variantsByGroup.set(item.group_id, groupVariants);
  }

  const reviewGroups = groups.map((group) => ({
    id: group.id,
    directory: group.directory,
    title: group.title,
    question: group.question,
    families: buildFamilies(variantsByGroup.get(group.id) ?? [])
  }));

  return {
    groups: reviewGroups,
    commentsPath
  };
}

// A variant during catalog assembly, carrying its family key and raw axes so families can be
// bucketed and their axis schema derived. The family/axes are dropped from the emitted `Variant`.
type GroupVariant = Variant & { family: string; axes: ManifestAxis[] };

// Bucket a group's variants into families (preserving manifest order), then derive each family's
// axis schema: distinct options per axis in first-seen order, with `varying` set when more than one
// option exists. Cross-axis sparsity is fine — only the coordinates that exist are emitted.
function buildFamilies(variants: GroupVariant[]): VariantFamily[] {
  const order: string[] = [];
  const byFamily = new Map<string, GroupVariant[]>();
  for (const variant of variants) {
    if (!byFamily.has(variant.family)) {
      byFamily.set(variant.family, []);
      order.push(variant.family);
    }
    byFamily.get(variant.family)!.push(variant);
  }

  return order.map((familyId) => {
    const members = byFamily.get(familyId)!;
    const axisOrder: string[] = [];
    const optionsByAxis = new Map<string, Map<string, string>>();
    for (const member of members) {
      for (const axis of member.axes) {
        if (!optionsByAxis.has(axis.axis)) {
          optionsByAxis.set(axis.axis, new Map());
          axisOrder.push(axis.axis);
        }
        const options = optionsByAxis.get(axis.axis)!;
        if (!options.has(axis.value)) {
          options.set(axis.value, axis.label);
        }
      }
    }
    const axes: VariantAxis[] = axisOrder.map((axisId) => {
      const options = [...optionsByAxis.get(axisId)!].map(([value, label]) => ({ value, label }));
      return {
        id: axisId,
        label: axisLabel(axisId),
        varying: options.length > 1,
        options
      };
    });
    const familyVariants: Variant[] = members.map((member) => ({
      coords: member.coords,
      case: member.case
    }));
    return { id: familyId, axes, variants: familyVariants };
  });
}

// When the rendered WAV was last written, read straight from the filesystem.
// Uses mtime (not birthtime): the renderer overwrites the WAV in place, so mtime
// is what advances on a re-render — exactly the "was this updated by an agent"
// signal. Returns null if the WAV is absent.
async function renderedAt(wav: string): Promise<string | null> {
  try {
    const stats = await stat(safeJoin(wavRoot, wav));
    return new Date(stats.mtimeMs).toISOString();
  } catch {
    return null;
  }
}

async function audioForCase(
  wav: string
): Promise<{ format: "wav"; relativePath: string; version: string }> {
  const stats = await stat(safeJoin(wavRoot, wav));
  return { format: "wav", relativePath: wav, version: mediaVersion(stats) };
}

function mediaUrl(relativePath: string, version: string): string {
  const encodedPath = relativePath
    .split("/")
    .map((segment) => encodeURIComponent(segment))
    .join("/");
  return `/media/wav/${encodedPath}?v=${encodeURIComponent(version)}`;
}

function mediaVersion(stats: { mtimeMs: number; size: number }): string {
  return `${Math.round(stats.mtimeMs)}-${stats.size}`;
}

async function readComments(): Promise<CommentsFile> {
  try {
    const parsed = JSON.parse(await readFile(commentsPath, "utf8")) as CommentsFile;
    return {
      schemaVersion: 2,
      updatedAt: parsed.updatedAt ?? new Date(0).toISOString(),
      comments: parsed.comments ?? {}
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
    comments: {}
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
