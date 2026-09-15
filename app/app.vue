<script setup>
const hive = useHive()
const { keySaved, keyModalOpen, selectedModel, chatModelIds } = hive

useHead({
  meta: [{ name: 'viewport', content: 'width=device-width, initial-scale=1' }],
  link: [{ rel: 'icon', href: '/favicon.ico' }],
  htmlAttrs: {
    lang: 'it'
  }
})

const title = 'ReQurv Launch'
const description = "Lancia i tuoi agenti AI già configurati per AI Hive, l'AI Gateway di ReQurv."

useSeoMeta({
  title,
  description,
  ogTitle: title,
  ogDescription: description,
  twitterCard: 'summary_large_image'
})

onMounted(async () => {
  await hive.loadSavedKey()
  hive.refreshStatus()
  if (!keySaved.value) {
    keyModalOpen.value = true
  }
})
</script>

<template>
  <UApp>
    <div class="flex min-h-svh flex-col">
      <UHeader>
        <template #left>
          <div class="flex min-w-0 items-center gap-3">
            <NuxtLink to="/" class="focus-visible:outline-3 outline-primary/25 rounded-md p-1 -ms-1">
              <AppLogo class="w-auto h-10 shrink-0" />
            </NuxtLink>

            <div class="min-w-0">
              <h1 class="text-sm font-semibold leading-tight">ReQurv Launch</h1>
              <p class="hidden truncate text-xs leading-tight text-muted md:block">
                Lancia i tuoi agenti AI già configurati per AI Hive, l'AI Gateway di ReQurv.
              </p>
            </div>
          </div>

          <UButton
            label="AI Hive"
            icon="i-lucide-external-link"
            color="neutral"
            variant="ghost"
            size="sm"
            class="ms-2 hidden md:inline-flex"
            @click="hive.openExternal('https://hive.requrv.ai')"
          />
        </template>

        <template #right>
          <USelect
            v-if="chatModelIds.length > 0"
            v-model="selectedModel"
            :items="chatModelIds"
            size="sm"
            aria-label="Modello di default"
            class="mr-2 w-44 lg:w-56"
          />
          <UButton
            :label="keySaved ? 'Chiave AI Hive' : 'Configura chiave'"
            icon="i-lucide-key-round"
            :trailing-icon="keySaved ? 'i-lucide-check' : undefined"
            :color="keySaved ? 'success' : 'warning'"
            :variant="keySaved ? 'subtle' : 'solid'"
            size="sm"
            class="mr-2"
            @click="keyModalOpen = true"
          />
          <UColorModeButton />
        </template>

        <template #bottom>
          <UContainer class="flex flex-wrap items-center justify-between gap-3 mt-2">
            <p class="text-xs text-dimmed">Hai appena installato un servizio? Esegui una nuova rilevazione.</p>
            <UButton
              icon="i-lucide-refresh-cw"
              label="Rileva di nuovo"
              color="neutral"
              variant="subtle"
              size="sm"
              :loading="refreshing"
              @click="hive.refreshStatus()"
            />
          </UContainer>
        </template>
      </UHeader>

      <UMain class="flex-1 min-h-0">
        <NuxtPage />
      </UMain>

      <USeparator icon="i-lucide-hexagon" />

      <UFooter>
        <template #left>
          <p class="text-sm text-muted">
            ReQurv Launch © {{ new Date().getFullYear() }} — powered by
            <button
              type="button"
              class="cursor-pointer text-primary hover:underline"
              @click="hive.openExternal('https://hive.requrv.ai')"
            >
              AI Hive
            </button>
          </p>
        </template>
      </UFooter>
    </div>

    <HiveKeyModal />
  </UApp>
</template>
