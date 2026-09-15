<script setup lang="ts">
const hive = useHive()
const { refreshing } = hive

onMounted(() => {
  hive.loadSavedKey()
  hive.refreshStatus()
})
</script>

<template>
  <div>
    <UPageHero
      title="ReQurv Launch"
      description="Lancia i tuoi agenti AI già configurati per AI Hive, l'AI Gateway di ReQurv."
      :links="[{
        label: 'Apri hive.requrv.ai',
        to: 'https://hive.requrv.ai',
        target: '_blank',
        trailingIcon: 'i-lucide-external-link',
        color: 'neutral',
        variant: 'subtle'
      }]"
    />

    <UPageSection>
      <HiveKeyPanel />
    </UPageSection>

    <UPageSection
      id="agents"
      title="Agenti"
      description="Ogni lancio configura automaticamente il servizio per usare AI Hive come provider e lo avvia in un nuovo terminale."
    >
      <div class="flex flex-wrap items-center justify-between gap-3">
        <p class="text-xs text-dimmed">
          Hai appena installato un servizio? Esegui una nuova rilevazione.
        </p>
        <UButton
          icon="i-lucide-refresh-cw"
          label="Rileva di nuovo"
          color="neutral"
          variant="subtle"
          size="sm"
          :loading="refreshing"
          @click="hive.refreshStatus()"
        />
      </div>

      <UGrid
        :cols="1"
        :min-cols="1"
        :max-cols="2"
        class="mt-4 gap-4"
      >
        <ServiceCard id="opencode" />
        <ServiceCard id="codex" />
      </UGrid>
    </UPageSection>
  </div>
</template>
