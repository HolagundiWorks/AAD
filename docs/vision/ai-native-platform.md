# AI-Native CAD Platform — Development Guidelines

> **Status:** Adopted as the long-term product direction (2026-07). See
> [ADR-0012](../adr/0012-ai-native-platform-pivot.md) for what this supersedes
> and [the phased roadmap](../plans/2026-07-ai-native-phases.md) for sequencing.
> Until a phase ships, the current locks in [ROADMAP](../ROADMAP.md) govern the
> shipping product.

## Vision

Develop an **AI-first engineering platform** where geometry, engineering
knowledge, enterprise data, and artificial intelligence coexist in a unified
architecture.

The platform should not merely create drawings. **It should understand them.**

Every engineering object must become an intelligent digital asset capable of
semantic reasoning, quantity extraction, validation, automation, and enterprise
integration.

## Core Principles

### 1. Geometry is the Source of Truth

- Never modify geometry through AI alone.
- Geometry must always remain mathematically accurate.
- Every line, arc, spline, hatch, block, and solid shall have deterministic
  coordinates.
- AI may **suggest** modifications, but the geometry engine **validates and
  commits** them.

### 2. Every Object is Intelligent

Every engineering entity shall contain:

- Unique Object ID
- Geometry
- Layer
- Metadata
- Engineering Properties
- Semantic Embedding
- Relationships
- Version History
- Permissions
- Audit Trail

Example — a **Wall**: ID · Geometry · Material · Thickness · Height ·
Fire Rating · Layer · Connected Rooms · Openings · Revision · Embedding.

### 3. Multi-Layer Data Architecture

The platform shall never rely on a single database.

| Store | Holds | Optimized for |
|---|---|---|
| **Geometry Database** | Lines, polylines, arcs, circles, splines, meshes, solids | Snapping, selection, editing, rendering |
| **Object Database** | Walls, doors, windows, beams, columns, slabs, reinforcement, text, dimensions | Typed engineering objects |
| **Metadata Database** | Project, discipline, drawing type, revision, status, client, company standards, material libraries | Drawing/project context |
| **Vector Database** | Embeddings for drawings, images, PDFs, objects, specifications, standards, photos, site reports | Semantic search, similarity search, AI context, RAG |
| **Graph Database** | Engineering relationships (Beam → *supports* → Slab · Door → *belongs to* → Wall · Column → *connected to* → Footing · Room → *contains* → Furniture) | Context and reasoning |
| **Spatial Index** | Bounding boxes, R-trees, quad-trees, BVH | Fast selection, collision, window selection, snapping |

### 4. AI Never Owns Data

AI is an assistant. **The database owns engineering data.**

AI may: suggest · predict · complete · validate · explain · generate.
But **the database commits changes.**

## Object Architecture

Every object shall contain: Geometry · Metadata · Engineering Properties ·
Relationships · Embeddings · Version · Audit · Permissions · Constraints ·
AI Confidence.

## Layer Standards

Layers are no longer colours only — **each layer becomes semantic.**

Example — `A-WALL` carries: wall type, fire rating, material, AI classifier,
company standards, default properties, drawing rules.

## Metadata Standards

Every drawing carries: project, client, revision, discipline, drawing number,
status, engineer, checker, approver, location, scale, issue date, company,
embedding, drawing summary, AI tags.

## Semantic Engine

Every object shall generate embeddings.

**Embedding types:** geometry, visual, text, engineering, relationship.

Embeddings are **regenerated after every approved modification.**

## AI Services

The AI layer shall provide: drawing completion · drawing classification ·
object detection · auto layer assignment · auto annotation · dimension
prediction · quantity extraction · specification search · code compliance ·
risk detection · duplicate detection · revision comparison · standard detail
recommendation · design assistant · natural-language commands.

### Implementation status (2026-08)

| Capability | Status |
|---|---|
| Auto layer assignment | ✅ `MAPMETA` (ADR-0010) |
| Object detection | ✅ M10 — `aadt-detect` → Object DB promotion |
| Duplicate detection | ✅ M11 — `FIND DUPES` |
| Specification search | ✅ M11 — `ASKDOCS` (retrieval + citation only) |
| Dimension prediction | ✅ M12 — `SUGGEST DIMS` |
| Code compliance | ✅ M12 — `CHECK` rule packs, advisory |
| Natural-language commands | ✅ M12 — `ASK` routes to a command *string* |
| Revision comparison | ✅ M12.6 — `COMPARE`, advisory narration |
| Standard detail recommendation | ✅ M12.7 — `RECOMMEND DETAIL` over `libraries/details/` |
| Quantity extraction | ✅ M13 — `TAKEOFF` / `BOQ` on objects |
| Drawing classification | ⚠️ Partial — element-level classification only |
| Auto annotation | ⚠️ Partial — dimensions only |
| Risk detection | ⬜ Future |
| Drawing completion · design assistant | ⬜ Future (would need generation, not retrieval) |

Everything shipped is **local, offline, and gated**: a service proposes and a
human commits (ADR-0013). The unshipped rows are the ones that would have to
generate content rather than retrieve or measure it — the deliberate frontier.

## CAD Engine

The CAD engine must remain **deterministic**.

Required: undo · redo · command stack · object history · precision editing ·
constraints · associative dimensions · blocks · dynamic blocks · XRefs ·
layouts · paper space · model space · rendering pipeline · selection engine ·
snapping engine · command recorder · macro support.

## Construction Intelligence

Every engineering object shall understand construction.

Example — a wall automatically knows: paint area, plaster area, brick quantity,
mortar, cost, labour, schedule, dependencies.

## Enterprise Integration

Every object may connect to: ERP · procurement · inventory · site progress ·
billing · BOQ · contracts · RFI · QA/QC · document management.

## Security

Every modification must be logged, storing: user, timestamp, machine, version,
previous state, new state, AI suggestion, approval, reason.

**Nothing is deleted. Everything is versioned.**

## Performance Targets

| Operation | Target |
|---|---|
| Drawing open | < 2 s |
| Zoom | 60 FPS |
| Selection | < 20 ms |
| Snap | < 5 ms |
| Search | < 100 ms |
| AI suggestion | < 2 s |
| Large projects | Millions of entities |

## Plugin Architecture

**Core:** geometry, rendering, database, AI assist (propose-only).
**Plugins:** ERP, estimating, document management, reports — registered as
**commands** over the local API (`CONNECTOR …`, M14.4 / ADR-0016), not as a
second write path into the document.

## API Philosophy

Everything available inside the UI shall also be available through the local
command-line API (`aadt serve` / `POST /v1/command` — ADR-0015). Long-term
vision still allows additional SDKs; they must compose command strings rather
than mutate entities directly. **Automation first, one author of geometry.**

## Future AI Vision

The long-term objective is an **Engineering Intelligence Platform** rather than
a CAD application. Users should be able to ask:

- "Complete this floor."
- "Estimate this building."
- "Find similar hospitals."
- "Show reinforcement used in similar projects."
- "Generate BOQ."
- "Check IS code compliance."
- "Predict missing dimensions."
- "Explain this detail."

The system should understand drawings, remember enterprise knowledge, learn
from approved projects, and continuously improve while maintaining
deterministic engineering accuracy.

## Development Philosophy

Every feature must satisfy four questions before release:

1. Is the geometry mathematically correct?
2. Does the engineering intent remain accurate?
3. Can the object be understood semantically by AI?
4. Can the feature scale to enterprise projects containing millions of objects?

If any answer is "No," the feature is not production-ready.

## Guiding Principle

> **"Geometry defines shape. Metadata defines meaning. Relationships define
> context. Vectors enable understanding. AI accelerates engineering. The
> database preserves truth."**
