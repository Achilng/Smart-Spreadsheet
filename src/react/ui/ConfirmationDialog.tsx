import { create } from "zustand";
import { Button, Modal } from "./controls";

const useConfirmation = create<{ message: string; resolve: ((value: boolean) => void) | null }>(() => ({ message: "", resolve: null }));
export function requestConfirmation(message: string): Promise<boolean> {
  if (useConfirmation.getState().resolve) return Promise.resolve(false);
  return new Promise(resolve => useConfirmation.setState({ message, resolve }));
}
export function ConfirmationDialog() {
  const { message, resolve } = useConfirmation();
  const answer = (value: boolean) => { useConfirmation.setState({ message: "", resolve: null }); resolve?.(value); };
  return <Modal open={Boolean(resolve)} title="确认操作" description={message} onClose={() => answer(false)}
    footer={<><Button onClick={() => answer(false)}>取消</Button><Button variant="primary" onClick={() => answer(true)}>确认</Button></>}>{null}</Modal>;
}
