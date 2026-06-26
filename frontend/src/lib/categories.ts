import type { Category, MarketProject } from "../types";

export function normalizeCategoryList(categories: Category[]) {
  const byId = new Map<string, Category>();
  for (const category of categories) {
    const id = category.id.trim();
    if (!id || id.startsWith("project:")) continue;
    const name = category.name.trim() || id;
    byId.set(id, {
      id,
      name,
      order: Number.isFinite(category.order) ? category.order : 0
    });
  }

  const normalized = [...byId.values()].sort((a, b) => {
    if (a.order !== b.order) return a.order - b.order;
    return a.id.localeCompare(b.id, "en");
  });

  let nextOrder = 10;
  return normalized.map((category) => {
    const order = category.order >= nextOrder ? category.order : nextOrder;
    nextOrder = order + 10;
    return { ...category, order };
  });
}

export function nextCategoryOrder(categories: Category[]) {
  return categories.reduce((max, category) => Math.max(max, category.order), 0) + 10;
}

export function projectOrder(project: MarketProject) {
  return Number.isFinite(project.order) ? project.order : 0;
}

export function compareMarketProjects(first: MarketProject, second: MarketProject) {
  const firstOrder = projectOrder(first);
  const secondOrder = projectOrder(second);
  if (firstOrder !== secondOrder) return firstOrder - secondOrder;
  return first.slug.localeCompare(second.slug, "en");
}

export function normalizeProjectList(projects: MarketProject[]) {
  const bySlug = new Map<string, MarketProject>();
  for (const project of projects) {
    const slug = project.slug.trim();
    if (!slug) continue;
    bySlug.set(slug, {
      ...project,
      slug,
      name: project.name.trim() || slug,
      description: project.description.trim(),
      order: projectOrder(project)
    });
  }

  const normalized = [...bySlug.values()].sort(compareMarketProjects);
  let nextOrder = 10;
  return normalized.map((project) => {
    const order = project.order >= nextOrder ? project.order : nextOrder;
    nextOrder = order + 10;
    return { ...project, order };
  });
}

export function nextProjectOrder(projects: MarketProject[]) {
  return projects.reduce((max, project) => Math.max(max, projectOrder(project)), 0) + 10;
}

export function categoryNameFromSlug(slug: string) {
  if (slug === "uncategorized") return "未分类";
  return slug
    .split(/[-_/]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toLocaleUpperCase() + part.slice(1))
    .join(" ");
}

export function emptyMarketProject(): MarketProject {
  return {
    slug: "",
    name: "",
    description: "",
    order: 10
  };
}

export function emptyMarketCategory(): Category {
  return {
    id: "",
    name: "",
    order: 10
  };
}
