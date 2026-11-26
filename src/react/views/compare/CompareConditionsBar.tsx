import { Popover } from "radix-ui";
import { ChevronDown } from "lucide-react";
import { compareFields, type CompareField, type CompareOptions } from "../../../lib/api/compare-conditions";
import { setCompareOptions } from "../../state/compare";
import { Button, Checkbox, Select } from "../../ui/controls";

const fields = Object.entries(compareFields) as [CompareField, string][];

function lockedSummary(locked: string[], others: number): string {
  if (!locked.length) return "其它条件都可以不同";
  if (locked.length === others) return "其它条件都要和 A 一致";
  return `${locked.slice(0, 2).join("、")}${locked.length > 2 ? ` 等 ${locked.length} 项` : ""}要和 A 一致`;
}

/** Custom comparison controls in the results header; every change re-runs the comparison. */
export function CompareConditionsBar({ options }: { options: CompareOptions }) {
  const update = (patch: Partial<CompareOptions>) => setCompareOptions({ ...options, ...patch });
  const others = fields.filter(([field]) => field !== options.observe);
  const locked = others.filter(([field]) => !options.allowed.includes(field)).map(([, label]) => label);
  return <div className="rc-conditions">
    <span className="rc-conditions-label">比较</span>
    <Select label="要比较什么" value={options.observe} options={fields} onChange={observe => update({ observe })} />
    <label className="ce-field" title="和 A 的差别完全相同的图合成一组，只显示一张"><Checkbox checked={options.dedupe} onCheckedChange={checked => update({ dedupe: checked === true })} />差别相同的合成一组</label>
    {options.dedupe && <Popover.Root>
      <Popover.Trigger asChild><Button size="sm" variant="ghost" className="rc-locked-trigger">{lockedSummary(locked, others.length)}<ChevronDown size={13} /></Button></Popover.Trigger>
      <Popover.Portal><Popover.Content className="rc-allowed-popover" align="start" sideOffset={6} collisionPadding={12} aria-label="哪些条件可以和 A 不同">
        <p>勾选的条件允许和 A 不同；没勾选的必须和 A 一致（两边都没记录也算一致）。</p>
        <div className="rc-allowed-list">{others.map(([field, label]) =>
          <label className="ce-field" key={field}><Checkbox checked={options.allowed.includes(field)} onCheckedChange={checked =>
            update({ allowed: checked ? [...options.allowed, field] : options.allowed.filter(value => value !== field) })} />{label}</label>)}
        </div>
        <div className="rc-allowed-actions">
          <Button size="sm" variant="ghost" onClick={() => update({ allowed: others.map(([field]) => field) })}>全部允许</Button>
          <Button size="sm" variant="ghost" onClick={() => update({ allowed: [] })}>全部要一致</Button>
        </div>
      </Popover.Content></Popover.Portal>
    </Popover.Root>}
  </div>;
}
