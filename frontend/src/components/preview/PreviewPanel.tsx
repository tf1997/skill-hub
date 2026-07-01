import React, { useEffect, useMemo, useState } from "react";
import {
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Copy,
  FileText,
  Folder,
  FolderOpen,
  Maximize2,
  Minimize2,
  Minus,
  RotateCcw,
  X,
  ZoomIn
} from "lucide-react";
import type { SkillPreview, SkillPreviewFileEntry } from "../../types";
import { Badge } from "../common/Badge";

const MIN_ZOOM = 0.78;
const MAX_ZOOM = 1.6;
const ZOOM_STEP = 0.1;

function clampZoom(value: number) {
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, Number(value.toFixed(2))));
}

function parentFoldersFor(path: string) {
  const parts = path.split("/");
  const folders: string[] = [];
  for (let index = 1; index < parts.length; index += 1) {
    folders.push(parts.slice(0, index).join("/"));
  }
  return folders;
}

export function PreviewPanel(props: { preview: SkillPreview; onSelectFile: (filePath: string) => void; onClose: () => void }) {
  const entries = useMemo<SkillPreviewFileEntry[]>(
    () =>
      props.preview.fileList?.length
        ? props.preview.fileList
        : props.preview.files.map((file) => ({
            path: file.path,
            language: file.language,
            previewable: true
          })),
    [props.preview]
  );
  const loadedFiles = useMemo(
    () => new Map(props.preview.files.map((file) => [file.path, file])),
    [props.preview.files]
  );
  const defaultPath = props.preview.files[0]?.path ?? entries[0]?.path ?? "";
  const [selectedPath, setSelectedPath] = useState(defaultPath);
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set());
  const [isExpanded, setIsExpanded] = useState(false);
  const [codeZoom, setCodeZoom] = useState(1);
  const [copyStatus, setCopyStatus] = useState("");

  const folders = useMemo(() => {
    const folderSet = new Set<string>();
    entries.forEach((entry) => {
      parentFoldersFor(entry.path).forEach((folder) => folderSet.add(folder));
    });
    return Array.from(folderSet).sort((first, second) => first.localeCompare(second));
  }, [entries]);

  const previewableEntries = useMemo(
    () => entries.filter((entry) => entry.previewable),
    [entries]
  );

  useEffect(() => {
    setSelectedPath((current) => {
      if (current && entries.some((entry) => entry.path === current)) {
        return current;
      }
      return defaultPath;
    });
  }, [defaultPath, entries]);

  useEffect(() => {
    if (!selectedPath) return;
    const parents = parentFoldersFor(selectedPath);
    if (parents.length === 0) return;
    setExpandedFolders((current) => {
      const next = new Set(current);
      parents.forEach((folder) => next.add(folder));
      return next;
    });
  }, [selectedPath]);

  useEffect(() => {
    if (!copyStatus) return;
    const timer = window.setTimeout(() => setCopyStatus(""), 1600);
    return () => window.clearTimeout(timer);
  }, [copyStatus]);

  const selectedEntry = entries.find((entry) => entry.path === selectedPath) ?? entries[0];
  const selectedFile = selectedEntry ? loadedFiles.get(selectedEntry.path) : undefined;
  const currentPreviewIndex = selectedEntry
    ? previewableEntries.findIndex((entry) => entry.path === selectedEntry.path)
    : -1;
  const canStepFiles = previewableEntries.length > 1;
  const codeStyle = {
    "--preview-code-font-size": `${Math.round(12 * codeZoom)}px`
  } as React.CSSProperties;

  function selectEntry(path: string, previewable: boolean) {
    setSelectedPath(path);
    if (previewable && !loadedFiles.has(path)) {
      props.onSelectFile(path);
    }
  }

  function selectAdjacentFile(direction: -1 | 1) {
    if (previewableEntries.length === 0) return;
    const currentIndex = previewableEntries.findIndex((entry) => entry.path === selectedPath);
    const baseIndex = currentIndex === -1 ? (direction > 0 ? -1 : 0) : currentIndex;
    const nextIndex = (baseIndex + direction + previewableEntries.length) % previewableEntries.length;
    const nextEntry = previewableEntries[nextIndex];
    if (nextEntry) {
      selectEntry(nextEntry.path, true);
    }
  }

  function toggleFolder(folderPath: string) {
    setExpandedFolders((current) => {
      const next = new Set(current);
      if (next.has(folderPath)) {
        next.delete(folderPath);
      } else {
        next.add(folderPath);
      }
      return next;
    });
  }

  function expandAllFolders() {
    setExpandedFolders(new Set(folders));
  }

  function collapseAllFolders() {
    setExpandedFolders(new Set());
  }

  function adjustZoom(delta: number) {
    setCodeZoom((current) => clampZoom(current + delta));
  }

  function resetZoom() {
    setCodeZoom(1);
  }

  async function copyText(text: string, label: string) {
    if (!text) return;
    try {
      if (!navigator.clipboard?.writeText) {
        throw new Error("Clipboard API is unavailable");
      }
      await navigator.clipboard.writeText(text);
      setCopyStatus(`${label}已复制`);
    } catch {
      setCopyStatus("复制失败，请手动复制");
    }
  }

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        props.onClose();
        return;
      }

      if (event.altKey && event.key === "ArrowLeft") {
        event.preventDefault();
        selectAdjacentFile(-1);
        return;
      }

      if (event.altKey && event.key === "ArrowRight") {
        event.preventDefault();
        selectAdjacentFile(1);
        return;
      }

      if (!(event.ctrlKey || event.metaKey)) return;

      if (event.key === "+" || event.key === "=") {
        event.preventDefault();
        adjustZoom(ZOOM_STEP);
      } else if (event.key === "-") {
        event.preventDefault();
        adjustZoom(-ZOOM_STEP);
      } else if (event.key === "0") {
        event.preventDefault();
        resetZoom();
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  });

  function getChildFolders(parentPath: string): string[] {
    const prefix = parentPath ? `${parentPath}/` : "";
    const depth = parentPath ? parentPath.split("/").length : 0;
    return folders.filter((folder) => {
      if (parentPath && !folder.startsWith(prefix)) return false;
      if (!parentPath && folder.includes("/")) return false;
      return folder.split("/").length === depth + 1;
    });
  }

  function getChildFiles(parentPath: string): SkillPreviewFileEntry[] {
    const prefix = parentPath ? `${parentPath}/` : "";
    return entries.filter((entry) => {
      if (!parentPath) return !entry.path.includes("/");
      if (!entry.path.startsWith(prefix)) return false;
      return !entry.path.slice(prefix.length).includes("/");
    });
  }

  function renderTree(folderPath: string, depth: number): React.ReactNode {
    const childFolders = getChildFolders(folderPath);
    const childFiles = getChildFiles(folderPath);
    const isFolderExpanded = expandedFolders.has(folderPath) || !folderPath;
    const folderName = folderPath ? folderPath.split("/").pop() : "";

    return (
      <React.Fragment key={folderPath || "root"}>
        {folderPath ? (
          <button
            type="button"
            className="preview-tree-folder"
            style={{ paddingLeft: `${12 + depth * 14}px` }}
            onClick={() => toggleFolder(folderPath)}
            aria-expanded={isFolderExpanded}
            title={folderPath}
          >
            {isFolderExpanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
            {isFolderExpanded ? <FolderOpen size={15} /> : <Folder size={15} />}
            <span>{folderName}</span>
            <small>{childFolders.length + childFiles.length} 项</small>
          </button>
        ) : null}
        {isFolderExpanded ? (
          <>
            {childFolders.map((childFolder) => renderTree(childFolder, depth + 1))}
            {childFiles.map((entry) => {
              const name = entry.path.split("/").pop() || entry.path;
              return (
                <button
                  type="button"
                  key={entry.path}
                  className={`preview-tree-item ${selectedEntry?.path === entry.path ? "active" : ""} ${
                    entry.previewable ? "" : "not-previewable"
                  }`}
                  style={{ paddingLeft: `${26 + (depth + 1) * 14}px` }}
                  onClick={() => selectEntry(entry.path, entry.previewable)}
                  title={entry.path}
                >
                  <FileText size={15} />
                  <span>{name}</span>
                  <Badge>{entry.previewable ? entry.language : "file"}</Badge>
                </button>
              );
            })}
          </>
        ) : null}
      </React.Fragment>
    );
  }

  return (
    <aside className={`preview-drawer ${isExpanded ? "expanded" : ""}`} role="dialog" aria-label="内容预览">
      <div className="preview-head">
        <div>
          <p>{props.preview.origin}</p>
          <h2>{props.preview.title}</h2>
          <span>{props.preview.rootPath}</span>
        </div>
        <div className="preview-window-actions">
          <button
            type="button"
            className="icon-button"
            onClick={() => setIsExpanded((current) => !current)}
            title={isExpanded ? "还原预览窗口" : "放大预览窗口"}
            aria-label={isExpanded ? "还原预览窗口" : "放大预览窗口"}
          >
            {isExpanded ? <Minimize2 size={17} /> : <Maximize2 size={17} />}
          </button>
          <button type="button" className="icon-button" onClick={props.onClose} title="关闭预览" aria-label="关闭预览">
            <X size={17} />
          </button>
        </div>
      </div>

      <div className="preview-toolbar">
        <div className="preview-file-nav" aria-label="文件切换">
          <button type="button" className="icon-button" onClick={() => selectAdjacentFile(-1)} disabled={!canStepFiles} title="上一个可预览文件">
            <ChevronLeft size={16} />
          </button>
          <span>{currentPreviewIndex >= 0 ? `${currentPreviewIndex + 1}/${previewableEntries.length}` : `${previewableEntries.length} 个可预览`}</span>
          <button type="button" className="icon-button" onClick={() => selectAdjacentFile(1)} disabled={!canStepFiles} title="下一个可预览文件">
            <ChevronRight size={16} />
          </button>
        </div>
        <div className="preview-tree-actions">
          <button type="button" className="primary-soft" onClick={expandAllFolders} disabled={folders.length === 0}>
            全展开
          </button>
          <button type="button" className="primary-soft" onClick={collapseAllFolders} disabled={folders.length === 0}>
            全收起
          </button>
        </div>
        <div className="preview-zoom-control" aria-label="代码字号缩放">
          <button type="button" className="icon-button" onClick={() => adjustZoom(-ZOOM_STEP)} disabled={codeZoom <= MIN_ZOOM} title="缩小内容">
            <Minus size={15} />
          </button>
          <span>{Math.round(codeZoom * 100)}%</span>
          <button type="button" className="icon-button" onClick={() => adjustZoom(ZOOM_STEP)} disabled={codeZoom >= MAX_ZOOM} title="放大内容">
            <ZoomIn size={15} />
          </button>
          <button type="button" className="icon-button" onClick={resetZoom} disabled={codeZoom === 1} title="重置缩放">
            <RotateCcw size={15} />
          </button>
        </div>
        {copyStatus ? <span className="preview-copy-status">{copyStatus}</span> : null}
      </div>

      <div className="preview-browser">
        {entries.length === 0 ? (
          <div className="empty-state">没有可预览的文件。</div>
        ) : (
          <>
            <aside className="preview-tree" aria-label="预览文件列表">
              <div className="preview-tree-summary">
                <FolderOpen size={16} />
                <strong>{entries.length} 个文件</strong>
              </div>
              <div className="preview-tree-list">{renderTree("", 0)}</div>
            </aside>

            <article className="preview-file">
              {selectedEntry ? (
                <>
                  <header className="preview-file-header">
                    <div className="preview-file-title">
                      <strong>{selectedEntry.path}</strong>
                      <span>
                        {selectedFile
                          ? selectedFile.truncated
                            ? "内容已截断"
                            : "完整预览"
                          : selectedEntry.previewable
                            ? "准备预览内容"
                            : "不可预览文件"}
                      </span>
                    </div>
                    <div className="preview-file-actions">
                      <Badge>{selectedEntry.language}</Badge>
                      {selectedFile?.truncated ? <Badge strong>截断</Badge> : null}
                      <button type="button" className="icon-button" onClick={() => copyText(selectedEntry.path, "路径")} title="复制文件路径">
                        <Copy size={15} />
                      </button>
                      <button
                        type="button"
                        className="icon-button"
                        onClick={() => copyText(selectedFile?.content ?? "", "内容")}
                        disabled={!selectedFile?.content}
                        title="复制文件内容"
                      >
                        <Copy size={15} />
                      </button>
                    </div>
                  </header>
                  {selectedFile ? (
                    <>
                      <pre style={codeStyle}>{selectedFile.content}</pre>
                      {selectedFile.truncated ? <small>内容过长，已截断预览。</small> : null}
                    </>
                  ) : (
                    <div className="preview-file-empty">
                      <strong>{selectedEntry.previewable ? "正在准备预览内容" : "该文件不是文本内容"}</strong>
                      <span>{selectedEntry.previewable ? "文件内容会在读取完成后显示。" : "可以复制路径，或在本地目录中打开查看。"}</span>
                    </div>
                  )}
                </>
              ) : (
                <div className="preview-file-empty">没有可预览的文本内容。</div>
              )}
            </article>
          </>
        )}
      </div>
    </aside>
  );
}
