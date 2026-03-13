import * as KButton from "@kobalte/core/button";
import { useNavigate } from "@tanstack/solid-router";
import { useI18n } from "../i18n/context";
import "./ErrorPage.css";

interface ErrorPageProps {
  error: Error;
  reset: () => void;
}

export function ErrorPage(props: ErrorPageProps) {
  const { t } = useI18n();
  const navigate = useNavigate();

  const errorMessage = () => {
    const err = props.error;
    if (err.message) return err.message;
    if ((err as any).cause) return String((err as any).cause);
    return String(err);
  };

  const getHint = () => {
    const msg = errorMessage().toLowerCase();
    if (msg.includes("network") || msg.includes("fetch") || msg.includes("connection")) {
      return "请检查网络连接或后端服务是否正常运行。";
    }
    if (msg.includes("tauri") || msg.includes("invoke")) {
      return "请确保应用已正确启动，可尝试重启应用。";
    }
    return t("common.errorPage.hint");
  };

  return (
    <div class="error-page">
      <div class="error-page-content">
        <div class="error-icon">⚠️</div>
        <h1 class="error-title">{t("common.errorPage.title")}</h1>
        <p class="error-message">{t("common.errorPage.message")}</p>

        <div class="error-actions">
          <KButton.Root class="kb-btn accent" onClick={props.reset}>
            {t("common.retry")}
          </KButton.Root>
          <KButton.Root class="kb-btn" onClick={() => navigate({ to: "/dashboard" })}>
            {t("common.errorPage.returnDashboard")}
          </KButton.Root>
        </div>

        <div class="error-hint">{getHint()}</div>

        <details class="error-details">
          <summary>{t("common.errorPage.details")}</summary>
          <pre class="error-stack">{props.error.stack || errorMessage()}</pre>
        </details>
      </div>
    </div>
  );
}
