import { Component, lazy, Suspense, type ReactNode } from "react";
import { useWorkspace } from "../../state/workspace";
import { Button } from "../../ui/controls";

const PromptDocs = lazy(() => import("./PromptDocsView").then(module => ({ default: module.PromptDocsView })));

class DocumentBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  render() {
    return this.state.failed ? <div className="r-state-message" role="alert" style={{ flex: 1 }}>
      <p>提示词文档未能打开，请重新打开应用后重试。</p>
      <Button onClick={() => useWorkspace.getState().setView("gallery")}>返回画廊</Button>
    </div> : this.props.children;
  }
}

export function DeferredPromptDocs() {
  return <DocumentBoundary><Suspense fallback={<div className="r-state-message" role="status" style={{ flex: 1 }}>正在打开提示词文档…</div>}>
    <PromptDocs />
  </Suspense></DocumentBoundary>;
}
