<script lang="ts">
  import AppShell from '$lib/components/AppShell.svelte';
  import ProjectList from '$lib/components/ProjectList.svelte';

  function startProject(): void {
    globalThis.location.assign('/projects/new');
  }
</script>

<svelte:head>
  <title>Azure SQL TCO</title>
  <meta
    name="description"
    content="Compare current SQL Server costs with an Azure SQL Managed Instance estimate."
  />
</svelte:head>

<AppShell>
  <ProjectList onnew={startProject} />
</AppShell>