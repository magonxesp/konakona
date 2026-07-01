import { Input } from '@/components/ui/input'
import './App.css'
import { Button, buttonVariants } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { Field, FieldLabel } from '@/components/ui/field'
import konata from '@/assets/konata.webp'
import { Circle, CircleX, DownloadIcon, LoaderCircle } from 'lucide-react'
import { useEffect, useState, type SubmitEventHandler } from 'react'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'

type DownloadFormat = 'Video' | 'Audio'
type Status = 'Enqueued' | 'InProgress' | 'Finished' | 'Failed'

type Download = {
  id: string,
  url: string,
  format: DownloadFormat,
  status: Status
}

type JobsStatus = {
  jobs: {
    [id: string]: Status
  }
}

function App() {
  const [submitting, setSubmitting] = useState(false)
  const [url, setUrl] = useState('')
  const [format, setFormat] = useState<DownloadFormat>('Video')
  const [downloads, setDownloads] = useState<Download[]>([])
  const [statusInterval, setStatusInterval] = useState<number | undefined>(undefined)

  useEffect(() => {
    const darkModeQuery = window.matchMedia('(prefers-color-scheme: dark)')
    const updateDarkMode = (matches: boolean) => {
      document.documentElement.classList.toggle('dark', matches)
    }
    const handleDarkModeChange = (event: MediaQueryListEvent) => {
      updateDarkMode(event.matches)
    }

    updateDarkMode(darkModeQuery.matches)
    darkModeQuery.addEventListener('change', handleDarkModeChange)

    return () => {
      darkModeQuery.removeEventListener('change', handleDarkModeChange)
    }
  }, [])

  const updateStatus = async () => {
    const response = await fetch('/api/status')
    const status = await response.json() as JobsStatus

    setDownloads(prev => prev.map(item => ({
      ...item,
      status: status.jobs[item.id]
    })))
  }

  const submitForm: SubmitEventHandler = (event) => {
    event.preventDefault()

    if (url === '') {
      return
    }

    setSubmitting(true)

    const download: Download = {
      id: crypto.randomUUID(),
      url: url,
      format,
      status: 'Enqueued'
    }

    fetch('/api/enqueue', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        id: download.id,
        url: download.url,
        format: download.format
      })
    })
      .then(() => {
        setUrl('')
        setDownloads(prev => [download, ...prev])
      })
      .finally(() => setSubmitting(false))
  }

  useEffect(() => {
    const hasCookie = document.cookie
      .split("; ")
      .some((cookie) => cookie.startsWith('SESSION='))

    if (!hasCookie) {
      document.cookie = `SESSION=${crypto.randomUUID()}`
    }
  }, [])

  useEffect(() => {
    const hasActiveDownloads = downloads.some(download => download.status !== 'Finished')
    const allFinished = downloads.filter(download => download.status === 'Finished' || download.status === 'Failed')

    if (downloads.length == 0) {
      return
    }

    if (hasActiveDownloads && statusInterval === undefined) {
      setStatusInterval(setInterval(() => updateStatus(), 1000))
      return
    }

    if (allFinished.length === downloads.length && statusInterval !== undefined) {
      clearInterval(statusInterval)
      setStatusInterval(undefined)
      return
    }
  }, [downloads])

  return (
    <main className="flex flex-col gap-3 max-w-lg mx-auto my-5">
      <section className="flex flex-col items-center px-3">
        <img className="h-50 md:h-80" src={konata} alt="" />
        <Card className="w-full">
          <CardContent>
            <form className="flex flex-col gap-3" onSubmit={submitForm}>
              <Tabs defaultValue="overview" onValueChange={value => setFormat(value)}>
                <TabsList>
                  <TabsTrigger value="Video">Video</TabsTrigger>
                  <TabsTrigger value="Audio">Music</TabsTrigger>
                </TabsList>
              </Tabs>
              <Field orientation="vertical">
                <FieldLabel>URL</FieldLabel>
                <Input
                  placeholder="https://www.youtube.com/watch?v=PYPZum8Pxvc"
                  type="text"
                  value={url}
                  onChange={event => setUrl(event.target.value)}
                />
              </Field>
              <Button disabled={submitting} type="submit">
                {submitting && <LoaderCircle className="animate-spin" />}
                Download
              </Button>
            </form>
          </CardContent>
        </Card>
      </section>
      {downloads.length > 0 && (
        <section className="px-3">
          <h2 className="scroll-m-20 text-xl font-semibold tracking-tight text-center mb-3 mt-6">
            Download queue
          </h2>
          <div className="flex flex-col gap-3">
            {downloads.map(item => (
              <Card className="w-full" key={item.id}>
                <CardContent className="flex items-center justify-between gap-3">
                  <span className="max-w-xs overflow-hidden text-ellipsis">{item.url}</span>
                  {item.status !== 'Finished' ? (
                    <>
                      <span className="text-sm text-muted-foreground">
                        {(item.status === 'Enqueued') ? 'waiting'
                          : (item.status === 'InProgress') ? 'in progress'
                            : (item.status === 'Failed') ? 'failed'
                              : (item.status === 'Finished') ? 'finish' : ''}
                      </span>
                      {(item.status === 'InProgress') ? (
                        <LoaderCircle className="animate-spin" />
                      ) : (item.status === 'Failed') ? (
                        <CircleX className="text-destructive" />
                      ) : (item.status === 'Enqueued') ? (
                        <Circle className="text-yellow-500" />
                      ) : ''}
                    </>
                  ) : (
                    <a
                      href={`/api/download/${item.id}`}
                      className={buttonVariants({ variant: 'default', size: 'sm' })}
                      target="_blank"
                      rel="noopener noreferrer"
                      download
                    >
                      <DownloadIcon />
                      Download
                    </a>
                  )}
                </CardContent>
              </Card>
            ))}
          </div>
        </section>
      )}
    </main>
  )
}

export default App
