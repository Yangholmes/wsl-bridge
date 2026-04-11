import { Match, Switch } from "solid-js";
import { useI18n } from "../i18n/context";
import { useAppRuntimeStatusQuery } from "./appRuntime";
import { Hint } from "./Hint";

export function AppRuntimeBanner() {
  const { t } = useI18n();
  const runtimeStatusQuery = useAppRuntimeStatusQuery();

  return (
    <Switch>
      <Match when={runtimeStatusQuery.data?.admin_features_available}>
        {null}
      </Match>
      <Match when={runtimeStatusQuery.data}>
        <Hint variant="warn" class="hint-banner">
          {t("app.runtimeNoticeBody")}
        </Hint>
      </Match>
    </Switch>
  );
}
