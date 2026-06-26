import { useEffect, useState } from "react";
import { AlertCircle, Archive, Blocks, BookOpen, CheckCircle2, ChevronDown, ChevronRight, Download, FileText, FolderGit2, FolderOpen, Layers3, PackageCheck, Pencil, Plus, RefreshCw, Rocket, Save, Search, Settings, Trash2, X } from "lucide-react";
import type { AdminAuditLog, AdminDraftPlugin, AdminDraftSkill, AdminSession, AppBootstrap, CachedPluginPackage, CachedSkillPackage, Category, LocalPlugin, LocalSkill, MarketPlugin, MarketProject, MarketSkill, Project, PublishMeta, SkillBinding, TargetRoot, UpdateCandidate } from "../../types";
import { draftCategoryLabel, draftPrimaryCategory, draftSearchText, draftSecondaryCategory, draftSkillLabel, draftStatusClass, draftStatusFilterLabels, draftStatusFilterOrder, sortDrafts } from "../../lib/adminDrafts";
import type { DraftStatusFilter, DraftStatusKey } from "../../lib/adminDrafts";

export function DraftList(props: {
  drafts: AdminDraftSkill[];
  selectedDraftPath: string | null;
  onSelectDraft: (draft: AdminDraftSkill) => void;
}) {
  const [collapsedCategories, setCollapsedCategories] = useState<Set<string>>(new Set());
  const [collapsedSubcategories, setCollapsedSubcategories] = useState<Set<string>>(new Set());
  const [draftQuery, setDraftQuery] = useState("");
  const [draftStatusFilter, setDraftStatusFilter] = useState<DraftStatusFilter>("all");

  const grouped = new Map<string, { direct: AdminDraftSkill[]; secondary: Map<string, AdminDraftSkill[]> }>();
  const normalizedQuery = draftQuery.trim().toLocaleLowerCase();
  const statusCounts = new Map<DraftStatusKey, number>();
  for (const draft of props.drafts) {
    const statusKey = draftStatusClass(draft);
    statusCounts.set(statusKey, (statusCounts.get(statusKey) ?? 0) + 1);
  }
  const activeStatusFilters = draftStatusFilterOrder.filter(
    (key) => (statusCounts.get(key) ?? 0) > 0 || draftStatusFilter === key
  );
  for (const draft of props.drafts) {
    const category = draftPrimaryCategory(draft);
    const secondary = draftSecondaryCategory(draft);
    const categoryText = draftCategoryLabel(category).toLocaleLowerCase();
    const secondaryText = secondary
      ? `${secondary} ${draftCategoryLabel(secondary)} ${category}/${secondary}`.toLocaleLowerCase()
      : "";
    const matchesQuery =
      !normalizedQuery ||
      categoryText.includes(normalizedQuery) ||
      secondaryText.includes(normalizedQuery) ||
      draftSearchText(draft).includes(normalizedQuery);
    const matchesStatus = draftStatusFilter === "all" || draftStatusClass(draft) === draftStatusFilter;
    if (!matchesQuery || !matchesStatus) {
      continue;
    }

    if (!grouped.has(category)) {
      grouped.set(category, { direct: [], secondary: new Map() });
    }
    const group = grouped.get(category)!;
    if (!secondary) {
      group.direct.push(draft);
      continue;
    }
    if (!group.secondary.has(secondary)) {
      group.secondary.set(secondary, []);
    }
    group.secondary.get(secondary)!.push(draft);
  }

  const categories = Array.from(grouped.keys()).sort();
  const secondaryKey = (category: string, secondary: string) => `${category}/${secondary}`;
  const allSecondaryKeys = categories.flatMap((category) =>
    Array.from(grouped.get(category)!.secondary.keys()).map((secondary) => secondaryKey(category, secondary))
  );
  const visibleDraftCount = categories.reduce((sum, category) => {
    const group = grouped.get(category)!;
    return (
      sum +
      group.direct.length +
      Array.from(group.secondary.values()).reduce((subtotal, drafts) => subtotal + drafts.length, 0)
    );
  }, 0);

  useEffect(() => {
    if (normalizedQuery || draftStatusFilter !== "all") {
      setCollapsedCategories(new Set());
      setCollapsedSubcategories(new Set());
    }
  }, [normalizedQuery, draftStatusFilter]);

  const toggleCategory = (category: string) => {
    const newSet = new Set(collapsedCategories);
    if (newSet.has(category)) {
      newSet.delete(category);
    } else {
      newSet.add(category);
    }
    setCollapsedCategories(newSet);
  };

  const toggleSubcategory = (key: string) => {
    const newSet = new Set(collapsedSubcategories);
    if (newSet.has(key)) {
      newSet.delete(key);
    } else {
      newSet.add(key);
    }
    setCollapsedSubcategories(newSet);
  };

  const expandAllDraftGroups = () => {
    setCollapsedCategories(new Set());
    setCollapsedSubcategories(new Set());
  };

  const collapseAllDraftGroups = () => {
    setCollapsedCategories(new Set(categories));
    setCollapsedSubcategories(new Set(allSecondaryKeys));
  };
  const allDraftGroupsCollapsed =
    categories.length > 0 && categories.every((category) => collapsedCategories.has(category));
  const toggleAllDraftGroups = () => {
    if (allDraftGroupsCollapsed) {
      expandAllDraftGroups();
    } else {
      collapseAllDraftGroups();
    }
  };

  const renderDraftRow = (draft: AdminDraftSkill, nested = false) => (
    <button
      type="button"
      key={draft.gitlabSourcePath}
      className={`draft-row ${nested ? "nested" : ""} ${props.selectedDraftPath === draft.gitlabSourcePath ? "active" : ""} ${!draft.sourceAvailable ? "no-source" : ""}`}
      onClick={() => props.onSelectDraft(draft)}
      title={!draft.sourceAvailable ? "未关联 GitLab 源，无法预览" : undefined}
    >
      <span className="draft-icon">
        <FileText size={16} />
      </span>
      <span className="draft-row-main">
        <strong>{draftSkillLabel(draft)}</strong>
      </span>
      <span className={`badge badge-status ${draftStatusClass(draft)}`}>
        {!draft.sourceAvailable && <AlertCircle size={12} className="badge-inline-icon" />}
        {draft.status}
      </span>
    </button>
  );

  return (
    <>
      <div className="draft-list-tools">
        <div className="search-box draft-search-box">
          <Search size={15} />
          <input
            value={draftQuery}
            onChange={(event) => setDraftQuery(event.target.value)}
            placeholder="搜索分类、二级分类或 skill"
            aria-label="搜索草稿分类和 skill"
          />
          {draftQuery ? (
            <button
              type="button"
              className="draft-search-clear"
              onClick={() => setDraftQuery("")}
              title="清空搜索"
              aria-label="清空搜索"
            >
              <X size={14} />
            </button>
          ) : null}
        </div>
        <div className="draft-list-actions">
          <span className="draft-list-count">
            {normalizedQuery || draftStatusFilter !== "all"
              ? `${visibleDraftCount}/${props.drafts.length}`
              : `${props.drafts.length}`}
          </span>
          <button
            type="button"
            className="draft-fold-button"
            onClick={toggleAllDraftGroups}
            disabled={categories.length === 0}
            aria-label={allDraftGroupsCollapsed ? "展开全部分类" : "折叠全部分类"}
            title={allDraftGroupsCollapsed ? "展开全部分类" : "折叠全部分类"}
          >
            {allDraftGroupsCollapsed ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
          </button>
        </div>
        <div className="draft-status-filter" aria-label="按状态过滤草稿">
          <button
            type="button"
            className={`draft-status-filter-button ${draftStatusFilter === "all" ? "active" : ""}`}
            onClick={() => setDraftStatusFilter("all")}
            aria-pressed={draftStatusFilter === "all"}
          >
            <span>{draftStatusFilterLabels.all}</span>
            <small>{props.drafts.length}</small>
          </button>
          {activeStatusFilters.map((key) => (
            <button
              type="button"
              key={key}
              className={`draft-status-filter-button ${draftStatusFilter === key ? "active" : ""}`}
              onClick={() => setDraftStatusFilter(key)}
              aria-pressed={draftStatusFilter === key}
            >
              <span>{draftStatusFilterLabels[key]}</span>
              <small>{statusCounts.get(key) ?? 0}</small>
            </button>
          ))}
        </div>
      </div>
      {categories.length === 0 ? (
        <div className="empty-state compact draft-empty-results">
          <strong>没有匹配的草稿</strong>
          <span>换个状态、分类、路径或 skill 名称试试。</span>
        </div>
      ) : null}
      {categories.map((category) => {
        const isCollapsed = collapsedCategories.has(category);
        const group = grouped.get(category)!;
        const secondaryCategories = Array.from(group.secondary.keys()).sort();
        const count = group.direct.length + secondaryCategories.reduce((sum, key) => sum + group.secondary.get(key)!.length, 0);
        return (
          <div key={category} className="draft-category-group">
            <button
              type="button"
              className={`draft-category-header ${isCollapsed ? "collapsed" : ""}`}
              onClick={() => toggleCategory(category)}
              aria-expanded={!isCollapsed}
            >
              <FolderGit2 size={18} />
              <strong className="draft-category-title">{draftCategoryLabel(category)}</strong>
              <span className="badge">{count}</span>
              <ChevronRight size={16} className="category-toggle" />
            </button>
            <div className={`draft-items ${isCollapsed ? "collapsed" : ""}`}>
              {group.direct.length > 0 ? (
                <div className="draft-direct-items">
                  {sortDrafts(group.direct).map((draft) => renderDraftRow(draft, true))}
                </div>
              ) : null}
              {secondaryCategories.map((secondary) => {
                const key = secondaryKey(category, secondary);
                const isSecondaryCollapsed = collapsedSubcategories.has(key);
                return (
                  <div key={key} className="draft-subcategory-group">
                    <button
                      type="button"
                      className={`draft-subcategory-label ${isSecondaryCollapsed ? "collapsed" : ""}`}
                      onClick={() => toggleSubcategory(key)}
                      aria-expanded={!isSecondaryCollapsed}
                    >
                      <FolderGit2 size={16} className="draft-subcategory-icon" />
                      <span className="draft-subcategory-title">{draftCategoryLabel(secondary)}</span>
                      <small>{group.secondary.get(secondary)!.length}</small>
                      <ChevronRight size={14} className="subcategory-toggle" />
                    </button>
                    <div className={`draft-subcategory-items ${isSecondaryCollapsed ? "collapsed" : ""}`}>
                      {sortDrafts(group.secondary.get(secondary)!).map((draft) => renderDraftRow(draft, true))}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        );
      })}
    </>
  );
}
