<script lang="ts">
  import { Check, ChevronDown, Search } from 'lucide-svelte';
  import type { RegionOption } from '$lib/regions';

  let {
    id,
    label,
    options,
    value = $bindable(),
    loading = false,
    required = false,
    onchange
  }: {
    id: string;
    label: string;
    options: RegionOption[];
    value: string | null;
    loading?: boolean;
    required?: boolean;
    onchange?: () => void;
  } = $props();

  let open = $state(false);
  let query = $state('');
  let activeIndex = $state(0);
  let input: HTMLInputElement;

  const selected = $derived(options.find((option) => option.value === value) ?? null);
  const filtered = $derived(
    options.filter((option) =>
      `${option.label} ${option.value}`.toLocaleLowerCase().includes(query.toLocaleLowerCase())
    )
  );
  const displayValue = $derived(open ? query : (selected?.label ?? value));

  function showOptions() {
    open = true;
    query = '';
    activeIndex = Math.max(
      0,
      options.findIndex((option) => option.value === value)
    );
  }

  function choose(option: RegionOption) {
    value = option.value;
    query = '';
    open = false;
    input.setCustomValidity('');
    onchange?.();
  }

  function search(event: Event) {
    query = (event.currentTarget as HTMLInputElement).value;
    open = true;
    activeIndex = 0;
    input.setCustomValidity('Choose a region from the list.');
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      open = false;
      query = '';
      return;
    }
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      if (!open) showOptions();
      const direction = event.key === 'ArrowDown' ? 1 : -1;
      activeIndex = Math.max(0, Math.min(filtered.length - 1, activeIndex + direction));
      return;
    }
    if (event.key === 'Enter' && open && filtered[activeIndex]) {
      event.preventDefault();
      choose(filtered[activeIndex]);
    }
  }

  function handleFocusOut(event: FocusEvent) {
    const container = event.currentTarget as HTMLElement;
    if (event.relatedTarget instanceof Node && container.contains(event.relatedTarget)) return;
    open = false;
    query = '';
    input.setCustomValidity(value ? '' : 'Choose a region from the list.');
  }
</script>

<label class="field-label" for={id}>{label}</label>
<div class="select" onfocusout={handleFocusOut}>
  <div class="input-wrap">
    {#if open}<Search class="leading" size={17} />{/if}
    <input
      bind:this={input}
      {id}
      role="combobox"
      aria-autocomplete="list"
      aria-controls={`${id}-options`}
      aria-expanded={open}
      aria-activedescendant={open && filtered[activeIndex] ? `${id}-${activeIndex}` : undefined}
      class:searching={open}
      value={displayValue}
      autocomplete="off"
      {required}
      onfocus={showOptions}
      oninput={search}
      onkeydown={handleKeydown}
    />
    <button
      type="button"
      class="toggle"
      aria-label={`${open ? 'Close' : 'Open'} ${label} options`}
      onclick={() => {
        if (open) {
          open = false;
          query = '';
        } else {
          input.focus();
          showOptions();
        }
      }}
    >
      <ChevronDown size={17} />
    </button>
  </div>

  {#if open}
    <ul id={`${id}-options`} role="listbox" aria-label={`${label} options`}>
      {#each filtered as option, index (option.value)}
        <li
          id={`${id}-${index}`}
          role="option"
          aria-selected={option.value === value}
          class:active={index === activeIndex}
        >
          <button
            type="button"
            onmousedown={(event) => event.preventDefault()}
            onclick={() => choose(option)}
          >
            <span><b>{option.label}</b><small>{option.value}</small></span>
            {#if option.value === value}<Check size={17} />{/if}
          </button>
        </li>
      {:else}
        <li class="empty">{loading ? 'Loading regions…' : 'No matching regions'}</li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .field-label {
    display: block;
    margin-bottom: 6px;
    color: #374e53;
    font-size: 0.78rem;
    font-weight: 700;
  }
  .select {
    position: relative;
  }
  .input-wrap {
    position: relative;
  }
  input {
    width: 100%;
    min-height: 40px;
    padding: 8px 38px 8px 10px;
    color: #162e33;
    background: #fff;
    border: 1px solid #96a7aa;
    border-radius: 4px;
    font:
      400 0.92rem/1.3 Aptos,
      'Trebuchet MS',
      sans-serif;
  }
  input.searching {
    padding-left: 36px;
  }
  input:focus {
    border-color: #087f73;
    outline: 2px solid #bae0d9;
  }
  :global(.leading) {
    position: absolute;
    z-index: 1;
    top: 12px;
    left: 11px;
    color: #627579;
    pointer-events: none;
  }
  .toggle {
    position: absolute;
    top: 1px;
    right: 1px;
    display: grid;
    width: 38px;
    height: 38px;
    padding: 0;
    place-items: center;
    color: #496167;
    background: transparent;
    border: 0;
    cursor: pointer;
  }
  ul {
    position: absolute;
    z-index: 20;
    top: calc(100% + 4px);
    right: 0;
    left: 0;
    max-height: 248px;
    margin: 0;
    padding: 4px;
    overflow-y: auto;
    list-style: none;
    background: #fff;
    border: 1px solid #96a7aa;
    border-radius: 4px;
    box-shadow: 0 8px 20px rgb(25 49 54 / 16%);
  }
  li button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    width: 100%;
    min-height: 44px;
    padding: 7px 9px;
    color: #223b40;
    background: transparent;
    border: 0;
    border-radius: 3px;
    text-align: left;
    cursor: pointer;
  }
  li.active button,
  li button:hover {
    background: #e7f3f0;
  }
  li span {
    display: grid;
    gap: 2px;
  }
  li b {
    font-size: 0.87rem;
  }
  li small {
    color: #627579;
    font-size: 0.73rem;
  }
  .empty {
    padding: 12px 9px;
    color: #627579;
    font-size: 0.82rem;
  }
</style>
