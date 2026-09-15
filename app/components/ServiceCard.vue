<script setup lang="ts">
const props = defineProps<{ id: ServiceId }>()

const hive = useHive()
const { keySaved, selectedModel, status, launching, keyModalOpen } = hive

const meta = computed(() => SERVICE_META[props.id])
const installed = computed(() => status.value?.[props.id] ?? false)
const canLaunch = computed(() => keySaved.value && !!selectedModel.value && installed.value)
const needsSetup = computed(() => !keySaved.value || !selectedModel.value)

const blockReason = computed<string | null>(() => {
  if (!keySaved.value) return 'Configura prima la chiave AI Hive.'
  if (!selectedModel.value) return 'Seleziona prima un modello.'
  if (!installed.value) return `Scarica e installa ${meta.value.title} per poterlo avviare.`
  return null
})
</script>

<template>
  <UCard :class="canLaunch ? 'ring-1 ring-primary/25' : ''" :ui="{ body: 'flex h-full flex-col gap-4' }">
    <div class="flex items-center justify-between gap-3">
      <div class="flex items-center gap-3">
        <div class="rounded-xl bg-elevated p-3">
          <UIcon :name="meta.icon" class="size-7" />
        </div>
        <h3 class="text-xl font-semibold">
          {{ meta.title }}
        </h3>
      </div>
      <UBadge :color="installed ? 'success' : 'neutral'" :variant="installed ? 'subtle' : 'outline'">
        <UIcon :name="installed ? 'i-lucide-check' : 'i-lucide-x'" class="size-3.5" />
        {{ installed ? 'Installato' : 'Non installato' }}
      </UBadge>
    </div>

    <p class="text-sm text-muted">
      {{ meta.description }}
    </p>

    <div class="mt-auto flex flex-col gap-2">
      <p v-if="selectedModel" class="text-xs text-dimmed">
        Modello: <span class="font-mono text-muted">{{ selectedModel }}</span>
      </p>

      <UButton
        v-if="!installed"
        :label="`Scarica ${meta.title}`"
        icon="i-lucide-download"
        size="lg"
        block
        @click="hive.openExternal(meta.downloadUrl)"
      />

      <UButton
        :label="launching === id ? 'Avvio in corso…' : 'Lancia con AI Hive'"
        icon="i-lucide-rocket"
        :loading="launching === id"
        :disabled="!canLaunch"
        size="lg"
        block
        @click="hive.launch(id)"
      />

      <div v-if="blockReason" class="flex justify-center">
        <UButton
          v-if="needsSetup"
          variant="link"
          size="xs"
          color="primary"
          :label="blockReason || ''"
          trailing-icon="i-lucide-chevron-right"
          @click="keyModalOpen = true"
        />
        <p v-else class="text-xs text-dimmed">
          {{ blockReason }}
        </p>
      </div>
    </div>
  </UCard>
</template>
