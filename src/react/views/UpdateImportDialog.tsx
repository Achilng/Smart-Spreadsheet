import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { runExistingImageUpdate } from "../state/import-actions";
import { useTasks } from "../state/tasks";
import { Button, Modal } from "../ui/controls";
import { errorText } from "../../lib/utils/format";

export function UpdateImportDialog({ onClose }: { onClose: () => void }) {
  const busy = useTasks(state => state.busy);
  const [choosing, setChoosing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const choose = async (archive: boolean) => {
    setChoosing(true); setError(null);
    try {
      const path = await open({ directory: !archive, multiple: false, title: archive ? "选择原压缩包（只更新，不新增）" : "选择原图片文件夹（只更新，不新增）", ...(archive ? { filters: [{ name: "压缩包", extensions: ["zip", "7z", "rar"] }] } : {}) });
      if (typeof path === "string") { onClose(); await runExistingImageUpdate(path); }
    } catch (failure) { setError(errorText(failure)); } finally { setChoosing(false); }
  };
  return <Modal open onClose={onClose} title="更新现有图片" description="重新读取原图中的 NovelAI 元数据，更新资料库中已有的记录。" width={520} busy={busy || choosing}
    footer={<><Button disabled={busy || choosing} onClick={onClose}>取消</Button><Button disabled={busy || choosing} onClick={() => void choose(true)}>选择压缩包</Button><Button variant="primary" disabled={busy || choosing} onClick={() => void choose(false)}>选择文件夹</Button></>}>
    <div className="r-explanation"><strong>只更新现有图片，不会追加新图片：</strong><ul><li>优先按原路径匹配；原图搬家后，继续按文件内容或完整 NovelAI 元数据匹配。</li><li>匹配到多条记录时不自动覆盖。</li><li>更新提示词、画师串和图片指纹，保留 Tag、分组和行 ID。</li><li>忽略新图片；缺失或读取失败的旧图片保留原数据。</li><li><strong>有图片被更新时，会清空撤销和重做记录。</strong></li></ul></div>
    {error && <p role="alert" className="r-field-error">{error}</p>}
  </Modal>;
}
