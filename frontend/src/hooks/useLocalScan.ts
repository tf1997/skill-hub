import type { Dispatch, SetStateAction } from "react";
import { useCallback, useRef } from "react";
import { api } from "../api";
import { readError } from "../lib/errors";
import type { AppBootstrap } from "../types";

type UseLocalScanOptions = {
  setBusy: Dispatch<SetStateAction<boolean>>;
  setError: Dispatch<SetStateAction<string | null>>;
  setData: Dispatch<SetStateAction<AppBootstrap>>;
  setNotice: Dispatch<SetStateAction<string>>;
};

export function useLocalScan({ setBusy, setError, setData, setNotice }: UseLocalScanOptions) {
  const localScanInFlightRef = useRef(false);

  const scanLocal = useCallback(
    async (options: { silent?: boolean } = {}) => {
      if (localScanInFlightRef.current) return;
      localScanInFlightRef.current = true;
      setBusy(true);
      setError(null);
      try {
        const [skills, plugins] = await Promise.all([api.scanLocalSkills(), api.scanLocalPlugins()]);
        setData((current) => ({ ...current, localSkills: skills, localPlugins: plugins }));
        if (!options.silent) {
          setNotice("本地 skill / plugin 已扫描");
        }
      } catch (err) {
        setError(readError(err));
      } finally {
        localScanInFlightRef.current = false;
        setBusy(false);
      }
    },
    [setBusy, setData, setError, setNotice]
  );

  return { scanLocal };
}
