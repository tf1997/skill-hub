import { useCallback, useEffect, useState } from "react";
import { api } from "../api";
import { normalizeProjectList } from "../lib/categories";
import { readError } from "../lib/errors";
import type { AppBootstrap } from "../types";

const emptyBootstrap: AppBootstrap = {
  sources: [],
  categories: [],
  skills: [],
  plugins: [],
  marketProjects: [],
  bindings: [],
  cachedPackages: [],
  pluginPackages: [],
  pluginBindings: [],
  localPlugins: [],
  localSkills: [],
  projects: [],
  targetRoots: [],
  updates: [],
  metadataSyncError: null
};

export function useBootstrap() {
  const [data, setData] = useState<AppBootstrap>(emptyBootstrap);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("正在载入 Skill Hub...");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      const next = await api.bootstrap();
      setData((current) => ({ ...current, ...next, marketProjects: normalizeProjectList(next.marketProjects) }));
      if (next.metadataSyncError) {
        setError(`市场元数据同步失败，显示本地缓存：${next.metadataSyncError}`);
        setNotice("已载入本地缓存");
      } else {
        setNotice("市场元数据已从 MinIO 同步");
      }
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }, []);

  const refreshCatalog = useCallback(async () => {
    setBusy(true);
    setError(null);
    setNotice("正在从 MinIO 拉取市场元数据...");
    try {
      const next = await api.refreshCatalog();
      setData((current) => ({ ...current, ...next, marketProjects: normalizeProjectList(next.marketProjects) }));
      setNotice("市场元数据已从 MinIO 同步");
    } catch (err) {
      setError(`市场索引刷新失败：${readError(err)}`);
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    async function init() {
      try {
        await load();
      } catch (err) {
        console.error("Failed to initialize:", err);
      } finally {
        setLoading(false);
      }
    }
    void init();
  }, [load]);

  return {
    data,
    setData,
    loading,
    notice,
    setNotice,
    busy,
    setBusy,
    error,
    setError,
    load,
    refreshCatalog
  };
}
