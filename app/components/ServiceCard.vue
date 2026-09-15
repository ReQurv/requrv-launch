<script setup lang="ts">
const props = defineProps<{ id: ServiceId }>()

const hive = useHive()
const { keySaved, selectedModel, status, launching } = hive

const meta = computed(() => SERVICE_META[props.id])
const installed = computed(() => status.value[props.id])
const canLaunch = computed(
  () => keySaved.value && !!selectedModel.value && installed.value
)

function blockReason(): string | null {
  if (!keySaved.value) return 'Salva prima la chiave AI Hive.'
  if (!selectedModel.value) return 'Seleziona un modello.'
  if (!installed.value) return 'Installa il servizio per poterlo avviare.'
  return null
}
</script>

<template>
  <UCard :ui="{ body: 'flex h-full flex-col gap-4' }">
    <div class="flex items-center justify-between gap-3">
      <div class="flex items-center gap-3">
        <div class="rounded-xl bg-elevated p-2.5">
          <UIcon
            :name="meta.icon"
            class="size-6"
          />
        </div>
        <h3 class="text-lg font-semibold">
          {{ meta.title }}
        </h3>
      </div>
      <UBadge
        :color="installed ? 'success' : 'neutral'"
        :variant="installed ? 'subtle' : 'outline'"
      >
        <UIcon
          :name="installed ? 'i-lucide-check' : 'i-lucide-x'"
          class="size-3.5"
        />
        {{ installed ? 'Installato' : 'Non installato' }}
      </UBadge>
    </div>

    <p class="text-sm text-muted">
      {{ meta.description }}
    </p>

    <p
      v-if="!installed"
      class="rounded-md bg-elevated px-3 py-2 font-mono text-xs text-dimmed"
    >
      {{ meta.installCommand }}
    </p>

    <div class="mt-auto">
      <UButton
        :label="launching === id ? 'Avvio in corso…' : 'Lancia con AI Hive'"
        :loading="launching === id"
        :disabled="!canLaunch"
        block
        @click="hive.launch(id)"
      />
      <p
        v-if="blockReason()"
        class="mt-2 text-xs text-dimmed"
      >
        {{ blockReason() }}
      </p>
    </div>
  </UCard>
</template>
