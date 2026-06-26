import React, { useEffect, useMemo, useState } from "react";
import { ChevronDown, ChevronRight, FileText, Folder, FolderOpen, X } from "lucide-react";
import type { SkillPreview } from "../../types";
import { Badge } from "../common/Badge";

export function PreviewPanel(props: { preview: SkillPreview; onSelectFile: (filePath: string) => void; onClose: () => void }) {
  const entries = useMemo(
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

  // 构建文件夹树：提取所有唯一的文件夹
  const folders = useMemo(() => {
    const folderSet = new Set<string>();
    entries.forEach((entry) => {
      const parts = entry.path.split("/");
      if (parts.length > 1) {
        // 添加所有父文件夹路径
        for (let i = 1; i < parts.length; i++) {
          folderSet.add(parts.slice(0, i).join("/"));
        }
      }
    });
    return Array.from(folderSet).sort();
  }, [entries]);

  // 获取文件夹的直接子文件夹
  function getChildFolders(parentPath: string): string[] {
    const prefix = parentPath ? parentPath + "/" : "";
    const depth = parentPath ? parentPath.split("/").length : 0;
    return folders.filter((f) => {
      if (parentPath && !f.startsWith(prefix)) return false;
      if (!parentPath && f.includes("/")) return false;
      const parts = f.split("/");
      return parts.length === depth + 1;
    });
  }

  // 获取文件夹的直接子文件
  function getChildFiles(parentPath: string): typeof entries {
    const prefix = parentPath ? parentPath + "/" : "";
    return entries.filter((entry) => {
      if (parentPath) {
        // 必须以父路径开头
        if (!entry.path.startsWith(prefix)) return false;
        // 去掉前缀后不能包含 /（即是直接子文件）
        const relativePath = entry.path.substring(prefix.length);
        return !relativePath.includes("/");
      } else {
        // 根目录：不包含 / 的文件
        return !entry.path.includes("/");
      }
    });
  }

  useEffect(() => {
    if (!selectedPath || !entries.some((entry) => entry.path === selectedPath)) {
      setSelectedPath(defaultPath);
    }
  }, [defaultPath, entries, selectedPath]);

  const selectedEntry = entries.find((entry) => entry.path === selectedPath) ?? entries[0];
  const selectedFile = selectedEntry ? loadedFiles.get(selectedEntry.path) : undefined;

  function selectEntry(path: string, previewable: boolean) {
    setSelectedPath(path);
    if (previewable && !loadedFiles.has(path)) {
      props.onSelectFile(path);
    }
  }

  function toggleFolder(folderPath: string) {
    setExpandedFolders((prev) => {
      const next = new Set(prev);
      if (next.has(folderPath)) {
        next.delete(folderPath);
      } else {
        next.add(folderPath);
      }
      return next;
    });
  }

  function renderTree(folderPath: string, depth: number): React.ReactNode {
    const childFolders = getChildFolders(folderPath);
    const childFiles = getChildFiles(folderPath);
    const isExpanded = expandedFolders.has(folderPath) || !folderPath; // 根目录默认展开
    const folderName = folderPath ? folderPath.split("/").pop() : "";

    return (
      <React.Fragment key={folderPath || "root"}>
        {folderPath && (
          <button
            className="preview-tree-folder"
            style={{ paddingLeft: `${12 + depth * 14}px` }}
            onClick={() => toggleFolder(folderPath)}
          >
            {isExpanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
            {isExpanded ? <FolderOpen size={15} /> : <Folder size={15} />}
            <span>{folderName}</span>
            <small className="muted">{childFiles.length} 个</small>
          </button>
        )}
        {isExpanded && (
          <>
            {childFolders.map((childFolder) => renderTree(childFolder, depth + 1))}
            {childFiles.map((entry) => {
              const name = entry.path.split("/").pop() || entry.path;
              return (
                <button
                  key={entry.path}
                  className={`preview-tree-item ${selectedEntry?.path === entry.path ? "active" : ""}`}
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
        )}
      </React.Fragment>
    );
  }

  return (
    <aside className="preview-drawer">
      <div className="preview-head">
        <div>
          <p>{props.preview.origin}</p>
          <h2>{props.preview.title}</h2>
          <span>{props.preview.rootPath}</span>
        </div>
        <button className="icon-button" onClick={props.onClose} title="关闭预览">
          <X size={17} />
        </button>
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
              <div className="preview-tree-list">
                {renderTree("", 0)}
              </div>
            </aside>

            <article className="preview-file">
              {selectedEntry ? (
                <>
                  <header>
                    <strong>{selectedEntry.path}</strong>
                    <Badge>{selectedEntry.language}</Badge>
                  </header>
                  {selectedFile ? (
                    <>
                      <pre>{selectedFile.content}</pre>
                      {selectedFile.truncated ? <small>内容过长，已截断预览。</small> : null}
                    </>
                  ) : (
                    <div className="preview-file-empty">
                      {selectedEntry.previewable ? "正在准备预览内容。" : "该文件不是文本内容。"}
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
