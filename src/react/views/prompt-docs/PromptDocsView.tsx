import { useCallback, useEffect, useRef, useState } from "react";
import { useLibrary } from "../../state/library";
import { initializePromptDocs } from "../../state/prompt-docs";
import { Button, Modal } from "../../ui/controls";
import { PromptDocList } from "./PromptDocList";
import { PromptDocEditor } from "./PromptDocEditor";
import "./prompt-docs.css";

export function PromptDocsView() {
  const directory = useLibrary(state => state.snapshot?.dataDirectory);
  const [discardOpen, setDiscardOpen] = useState(false);
  const discardResolver = useRef<((answer: boolean) => void) | null>(null);
  const confirmDiscard = useCallback(() => new Promise<boolean>(resolve => {
    discardResolver.current = resolve; setDiscardOpen(true);
  }), []);
  const resolveDiscard = (answer: boolean) => {
    setDiscardOpen(false); discardResolver.current?.(answer); discardResolver.current = null;
  };
  useEffect(() => { void initializePromptDocs(); }, [directory]);
  useEffect(() => () => { discardResolver.current?.(false); discardResolver.current = null; }, []);
  return <section className="r-prompt-docs" aria-label="提示词文档">
    <PromptDocList confirmDiscard={confirmDiscard} />
    <PromptDocEditor confirmDiscard={confirmDiscard} />
    <Modal open={discardOpen} onClose={() => resolveDiscard(false)} title="当前文档保存失败" description="切换将丢失未保存的修改。你可以留在本文档重试保存。"
      footer={<><Button onClick={() => resolveDiscard(false)}>留在本文档</Button><Button variant="danger" onClick={() => resolveDiscard(true)}>放弃修改并切换</Button></>}><p>是否放弃当前修改？</p></Modal>
  </section>;
}
