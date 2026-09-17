<script setup lang="ts">
const hive = useHive()
const { launchTarget, launchModalOpen } = hive

const meta = computed(() => (launchTarget.value ? SERVICE_META[launchTarget.value] : null))

const appDescription = computed(() =>
  launchTarget.value === 'codex'
    ? 'Configura ChatGPT.app su AI Hive e aprila. I modelli dell\'account ChatGPT non saranno disponibili finché non ripristini la configurazione.'
    : 'Apri OpenCode.app con il provider ReQurv Hive configurato.'
)

const terminalDescription = computed(() =>
  launchTarget.value === 'codex'
    ? 'Avvia codex in un nuovo terminale con --profile hive.'
    : 'Avvia la CLI opencode in un nuovo terminale, già puntata ad AI Hive.'
)

async function choose(mode: LaunchMode) {
  const target = launchTarget.value
  launchModalOpen.value = false
  if (!target) return
  await hive.launch(target, mode)
}
</script>

<template>
  <UModal
    :open="launchModalOpen"
    :title="meta ? `Avvia ${meta.title}` : ''"
    description="Scegli come aprire l'agente."
    :ui="{ content: 'max-w-md' }"
    @update:open="launchModalOpen = $event"
  >
    <template #body>
      <div class="flex flex-col gap-4">
        <div>
          <UButton
            icon="i-lucide-app-window"
            size="lg"
            block
            variant="outline"
            label="Apri l'app"
            @click="choose('app')"
          />
          <p class="mt-1.5 text-center text-xs text-dimmed">
            {{ appDescription }}
          </p>
        </div>
        <div>
          <UButton
            icon="i-lucide-terminal"
            size="lg"
            block
            variant="outline"
            label="Apri il terminale"
            @click="choose('terminal')"
          />
          <p class="mt-1.5 text-center text-xs text-dimmed">
            {{ terminalDescription }}
          </p>
        </div>
      </div>
    </template>

    <template #footer>
      <div class="flex items-center justify-end gap-3">
        <UButton
          label="Annulla"
          color="neutral"
          variant="ghost"
          @click="launchModalOpen = false"
        />
      </div>
    </template>
  </UModal>
</template>
