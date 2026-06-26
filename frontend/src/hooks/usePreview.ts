import type { Dispatch, SetStateAction } from "react";
import { useCallback, useState } from "react";
import { api } from "../api";
import { readError } from "../lib/errors";
import type { CachedSkillItem } from "../lib/localSkills";
import { localPluginDisplayName } from "../lib/localSkills";
import type {
  AdminDraftPreviewRequest,
  AppBootstrap,
  LocalSkill,
  MarketPlugin,
  MarketSkill,
  PluginPreviewRequest,
  SkillBinding,
  SkillPreview,
  SkillPreviewRequest
} from "../types";

export type PreviewContext =
  | { kind: "skill"; request: SkillPreviewRequest }
  | { kind: "plugin"; request: PluginPreviewRequest }
  | { kind: "adminDraft"; request: AdminDraftPreviewRequest }
  | { kind: "adminPluginDraft"; request: AdminDraftPreviewRequest };

type UsePreviewOptions = {
  installTarget: string;
  setBusy: Dispatch<SetStateAction<boolean>>;
  setError: Dispatch<SetStateAction<string | null>>;
  setNotice: Dispatch<SetStateAction<string>>;
};

export function usePreview({ installTarget, setBusy, setError, setNotice }: UsePreviewOptions) {
  const [preview, setPreview] = useState<SkillPreview | null>(null);
  const [previewContext, setPreviewContext] = useState<PreviewContext | null>(null);

  const previewMarketSkill = useCallback(
    async (skill: MarketSkill) => {
      setBusy(true);
      setError(null);
      try {
        const request: SkillPreviewRequest = {
          sourceId: skill.sourceId,
          namespace: skill.namespace,
          skillId: skill.id,
          version: null
        };
        const result = await api.previewSkill(request);
        setPreview(result);
        setPreviewContext({ kind: "skill", request });
        setNotice(`正在预览 ${skill.name}`);
      } catch (err) {
        setError(readError(err));
      } finally {
        setBusy(false);
      }
    },
    [setBusy, setError, setNotice]
  );

  const previewMarketPlugin = useCallback(
    async (plugin: MarketPlugin) => {
      setBusy(true);
      setError(null);
      try {
        const target = plugin.targets.includes(installTarget) ? installTarget : plugin.targets[0] ?? "codex";
        const request: PluginPreviewRequest = {
          sourceId: plugin.sourceId,
          namespace: plugin.namespace,
          pluginId: plugin.id,
          version: null,
          target
        };
        const result = await api.previewPlugin(request);
        setPreview(result);
        setPreviewContext({ kind: "plugin", request });
        setNotice(`正在预览 ${plugin.name}`);
      } catch (err) {
        setError(readError(err));
      } finally {
        setBusy(false);
      }
    },
    [installTarget, setBusy, setError, setNotice]
  );

  const previewCachedSkill = useCallback(
    async (item: CachedSkillItem) => {
      setBusy(true);
      setError(null);
      try {
        const request: SkillPreviewRequest = {
          sourceId: item.package.sourceId,
          namespace: item.package.namespace,
          skillId: item.package.skillId,
          version: item.package.version
        };
        const result = await api.previewSkill(request);
        setPreview(result);
        setPreviewContext({ kind: "skill", request });
        setNotice(`正在预览 ${item.package.skillName} ${item.package.version}`);
      } catch (err) {
        setError(readError(err));
      } finally {
        setBusy(false);
      }
    },
    [setBusy, setError, setNotice]
  );

  const previewBinding = useCallback(
    async (binding: SkillBinding) => {
      setBusy(true);
      setError(null);
      try {
        const request: SkillPreviewRequest = { bindingId: binding.id };
        const result = await api.previewSkill(request);
        setPreview(result);
        setPreviewContext({ kind: "skill", request });
        setNotice(`正在预览 ${binding.skillName}`);
      } catch (err) {
        setError(readError(err));
      } finally {
        setBusy(false);
      }
    },
    [setBusy, setError, setNotice]
  );

  const previewLocalSkill = useCallback(
    async (skill: LocalSkill) => {
      setBusy(true);
      setError(null);
      try {
        const request: SkillPreviewRequest = { path: skill.path };
        const result = await api.previewSkill(request);
        setPreview(result);
        setPreviewContext({ kind: "skill", request });
        setNotice(`正在预览 ${skill.detectedManifest ?? skill.path}`);
      } catch (err) {
        setError(readError(err));
      } finally {
        setBusy(false);
      }
    },
    [setBusy, setError, setNotice]
  );

  const previewPluginBinding = useCallback(
    async (binding: AppBootstrap["pluginBindings"][number]) => {
      setBusy(true);
      setError(null);
      try {
        const request: PluginPreviewRequest = { bindingId: binding.id };
        const result = await api.previewPlugin(request);
        setPreview(result);
        setPreviewContext({ kind: "plugin", request });
        setNotice(`正在预览 ${binding.pluginName}`);
      } catch (err) {
        setError(readError(err));
      } finally {
        setBusy(false);
      }
    },
    [setBusy, setError, setNotice]
  );

  const previewCachedPlugin = useCallback(
    async (item: AppBootstrap["pluginPackages"][number]) => {
      setBusy(true);
      setError(null);
      try {
        const request: PluginPreviewRequest = {
          sourceId: item.sourceId,
          namespace: item.namespace,
          pluginId: item.pluginId,
          version: item.version,
          target: item.target,
          path: item.packagePath
        };
        const result = await api.previewPlugin(request);
        setPreview(result);
        setPreviewContext({ kind: "plugin", request });
        setNotice(`正在预览 ${item.pluginName} ${item.version}`);
      } catch (err) {
        setError(readError(err));
      } finally {
        setBusy(false);
      }
    },
    [setBusy, setError, setNotice]
  );

  const previewLocalPlugin = useCallback(
    async (plugin: AppBootstrap["localPlugins"][number]) => {
      setBusy(true);
      setError(null);
      try {
        const request: PluginPreviewRequest = { path: plugin.path, target: plugin.target };
        const result = await api.previewPlugin(request);
        setPreview(result);
        setPreviewContext({ kind: "plugin", request });
        setNotice(`正在预览 ${localPluginDisplayName(plugin)}`);
      } catch (err) {
        setError(readError(err));
      } finally {
        setBusy(false);
      }
    },
    [setBusy, setError, setNotice]
  );

  const loadPreviewFile = useCallback(
    async (filePath: string) => {
      if (!previewContext) {
        return;
      }
      setBusy(true);
      setError(null);
      try {
        if (previewContext.kind === "adminDraft") {
          const request = { ...previewContext.request, filePath };
          const result = await api.previewAdminDraft(request);
          setPreview(result);
          setPreviewContext({ kind: "adminDraft", request });
        } else if (previewContext.kind === "adminPluginDraft") {
          const request = { ...previewContext.request, filePath };
          const result = await api.previewAdminPluginDraft(request);
          setPreview(result);
          setPreviewContext({ kind: "adminPluginDraft", request });
        } else if (previewContext.kind === "plugin") {
          const request = { ...previewContext.request, filePath };
          const result = await api.previewPlugin(request);
          setPreview(result);
          setPreviewContext({ kind: "plugin", request });
        } else {
          const request = { ...previewContext.request, filePath };
          const result = await api.previewSkill(request);
          setPreview(result);
          setPreviewContext({ kind: "skill", request });
        }
      } catch (err) {
        setError(readError(err));
      } finally {
        setBusy(false);
      }
    },
    [previewContext, setBusy, setError]
  );

  return {
    preview,
    setPreview,
    previewContext,
    setPreviewContext,
    previewMarketSkill,
    previewMarketPlugin,
    previewCachedSkill,
    previewBinding,
    previewLocalSkill,
    previewPluginBinding,
    previewCachedPlugin,
    previewLocalPlugin,
    loadPreviewFile
  };
}
