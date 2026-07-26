<script lang="ts">
  import type { NumericComparison, NumericOperator } from "../../api";

  let {
    comparison,
    unit = "",
    onchange,
  }: {
    comparison: NumericComparison;
    unit?: string;
    onchange: (comparison: NumericComparison) => void;
  } = $props();

  function numberValue(event: Event): number {
    const value = Number((event.currentTarget as HTMLInputElement).value);
    return Number.isFinite(value) ? value : 0;
  }
</script>

<div class="numeric-editor">
  <select
    aria-label="比较方式"
    value={comparison.operator}
    onchange={event => onchange({
      ...comparison,
      operator: (event.currentTarget as HTMLSelectElement).value as NumericOperator,
      secondValue: (event.currentTarget as HTMLSelectElement).value === "between"
        ? (comparison.secondValue ?? comparison.value)
        : null,
    })}
  >
    <option value="equal">等于</option>
    <option value="notEqual">不等于</option>
    <option value="greaterThan">大于</option>
    <option value="greaterOrEqual">大于或等于</option>
    <option value="lessThan">小于</option>
    <option value="lessOrEqual">小于或等于</option>
    <option value="between">介于（含边界）</option>
  </select>
  <label>
    <span>{comparison.operator === "between" ? "起始值" : "数值"}</span>
    <input
      type="number"
      step="any"
      value={comparison.value}
      oninput={event => onchange({ ...comparison, value: numberValue(event) })}
    />
  </label>
  {#if comparison.operator === "between"}
    <span class="range-separator">至</span>
    <label>
      <span>结束值</span>
      <input
        type="number"
        step="any"
        value={comparison.secondValue ?? comparison.value}
        oninput={event => onchange({ ...comparison, secondValue: numberValue(event) })}
      />
    </label>
  {/if}
  {#if unit}<span class="unit">{unit}</span>{/if}
</div>

<style>
  .numeric-editor {
    display: flex;
    align-items: end;
    flex-wrap: wrap;
    gap: 8px;
  }

  label {
    display: grid;
    gap: 4px;
  }

  label span {
    color: var(--text-3);
    font-size: var(--font-xs);
  }

  input {
    min-height: 32px;
    padding: 5px 9px;
    width: 130px;
  }

  .range-separator,
  .unit {
    min-height: 34px;
    display: inline-flex;
    align-items: center;
    color: var(--text-3);
    font-size: var(--font-sm);
  }
</style>
