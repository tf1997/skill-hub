import type { Dispatch, SetStateAction } from "react";
import { useCallback, useState } from "react";
import { api } from "../api";
import type { AdminTab } from "../app/viewModel";
import { defaultMetaFromDraft, defaultMetaFromPluginDraft } from "../lib/adminDrafts";
import { readError } from "../lib/errors";
import type { AdminAuditLog, AdminDraftPlugin, AdminDraftSkill, AdminSession, MarketProject, PublishMeta } from "../types";

type UseAdminDataOptions = {
  adminKey: string;
  adminTab: AdminTab;
  setAdminTab: Dispatch<SetStateAction<AdminTab>>;
  marketProjects: MarketProject[];
  setBusy: Dispatch<SetStateAction<boolean>>;
  setError: Dispatch<SetStateAction<string | null>>;
  setNotice: Dispatch<SetStateAction<string>>;
  setDraftMeta: Dispatch<SetStateAction<PublishMeta>>;
  setPluginDraftMeta: Dispatch<SetStateAction<PublishMeta>>;
};

export function useAdminData({
  adminKey,
  adminTab,
  setAdminTab,
  marketProjects,
  setBusy,
  setError,
  setNotice,
  setDraftMeta,
  setPluginDraftMeta
}: UseAdminDataOptions) {
  const [adminSession, setAdminSession] = useState<AdminSession | null>(null);
  const [adminDrafts, setAdminDrafts] = useState<AdminDraftSkill[]>([]);
  const [adminPluginDrafts, setAdminPluginDrafts] = useState<AdminDraftPlugin[]>([]);
  const [adminAuditLogs, setAdminAuditLogs] = useState<AdminAuditLog[]>([]);
  const [selectedDraftPath, setSelectedDraftPath] = useState<string | null>(null);
  const [selectedPluginDraftPath, setSelectedPluginDraftPath] = useState<string | null>(null);

  const selectDraft = useCallback(
    (draft: AdminDraftSkill) => {
      setSelectedDraftPath(draft.gitlabSourcePath);
      const nextMeta = draft.publishMeta ?? defaultMetaFromDraft(draft);
      if (adminSession?.role === "project" && nextMeta.publishScope !== "project") {
        setDraftMeta({
          ...nextMeta,
          publishScope: "project",
          publishCategorySlug: null,
          publishProjectSlug: marketProjects[0]?.slug ?? null
        });
        return;
      }
      setDraftMeta(nextMeta);
    },
    [adminSession?.role, marketProjects, setDraftMeta]
  );

  const selectPluginDraft = useCallback(
    (draft: AdminDraftPlugin) => {
      setSelectedPluginDraftPath(draft.gitlabSourcePath);
      const nextMeta = draft.publishMeta ?? defaultMetaFromPluginDraft(draft);
      if (adminSession?.role === "project" && nextMeta.publishScope !== "project") {
        setPluginDraftMeta({
          ...nextMeta,
          publishScope: "project",
          publishCategorySlug: null,
          publishProjectSlug: marketProjects[0]?.slug ?? null
        });
        return;
      }
      setPluginDraftMeta(nextMeta);
    },
    [adminSession?.role, marketProjects, setPluginDraftMeta]
  );

  const refreshAdminDrafts = useCallback(async () => {
    if (!adminKey.trim()) {
      setError("请先输入管理员密钥");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const drafts = await api.listAdminDrafts(adminKey);
      setAdminDrafts(drafts);
      if (!selectedDraftPath && drafts.length > 0) {
        selectDraft(drafts[0]);
      }
      setNotice("草稿区已刷新");
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }, [adminKey, selectDraft, selectedDraftPath, setBusy, setError, setNotice]);

  const refreshAdminPluginDrafts = useCallback(async () => {
    if (!adminKey.trim()) {
      setError("Please enter admin key first");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const drafts = await api.listAdminPluginDrafts(adminKey);
      setAdminPluginDrafts(drafts);
      if (!selectedPluginDraftPath && drafts.length > 0) {
        selectPluginDraft(drafts[0]);
      }
      setNotice("Plugin 草稿区已刷新");
    } catch (err) {
      setError(readError(err));
    } finally {
      setBusy(false);
    }
  }, [adminKey, selectPluginDraft, selectedPluginDraftPath, setBusy, setError, setNotice]);

  const refreshAdminAuditLogs = useCallback(
    async (showBusy = true) => {
      if (adminSession && adminSession.role !== "system") {
        setAdminAuditLogs([]);
        return;
      }
      if (!adminKey.trim()) {
        setError("请先输入管理员密钥");
        return;
      }
      if (showBusy) {
        setBusy(true);
      }
      setError(null);
      try {
        const logs = await api.listAdminAuditLogs(adminKey, 100);
        setAdminAuditLogs(logs);
        if (showBusy) {
          setNotice("审计记录已刷新");
        }
      } catch (err) {
        setError(readError(err));
      } finally {
        if (showBusy) {
          setBusy(false);
        }
      }
    },
    [adminKey, adminSession, setBusy, setError, setNotice]
  );

  return {
    adminSession,
    setAdminSession,
    adminDrafts,
    setAdminDrafts,
    adminPluginDrafts,
    setAdminPluginDrafts,
    adminAuditLogs,
    setAdminAuditLogs,
    selectedDraftPath,
    setSelectedDraftPath,
    selectedPluginDraftPath,
    setSelectedPluginDraftPath,
    selectDraft,
    selectPluginDraft,
    refreshAdminDrafts,
    refreshAdminPluginDrafts,
    refreshAdminAuditLogs
  };
}
