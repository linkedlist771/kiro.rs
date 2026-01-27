import { useState } from 'react'
import { RefreshCw, LogOut, Moon, Sun, Server, Plus } from 'lucide-react'
import { useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { storage } from '@/lib/storage'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent } from '@/components/ui/card'
import { CredentialTable } from '@/components/credential-table'
import { BalanceDialog } from '@/components/balance-dialog'
import { AddCredentialDialog } from '@/components/add-credential-dialog'
import { useCredentials } from '@/hooks/use-credentials'

interface DashboardProps {
  onLogout: () => void
}

export function Dashboard({ onLogout }: DashboardProps) {
  const [selectedCredentialId, setSelectedCredentialId] = useState<number | null>(null)
  const [balanceDialogOpen, setBalanceDialogOpen] = useState(false)
  const [addDialogOpen, setAddDialogOpen] = useState(false)
  const [darkMode, setDarkMode] = useState(() => {
    if (typeof window !== 'undefined') {
      return document.documentElement.classList.contains('dark')
    }
    return false
  })

  const queryClient = useQueryClient()
  const { data, isLoading, error, refetch } = useCredentials()

  const toggleDarkMode = () => {
    setDarkMode(!darkMode)
    document.documentElement.classList.toggle('dark')
  }

  const handleViewBalance = (id: number) => {
    setSelectedCredentialId(id)
    setBalanceDialogOpen(true)
  }

  const handleRefresh = () => {
    refetch()
    toast.success('已刷新凭据列表')
  }

  const handleLogout = () => {
    storage.removeApiKey()
    queryClient.clear()
    onLogout()
  }

  if (isLoading) {
    return (
      <div className="min-h-screen flex items-center justify-center">
        <div className="text-center">
          <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-primary mx-auto mb-4"></div>
          <p className="text-muted-foreground">加载中...</p>
        </div>
      </div>
    )
  }

  if (error) {
    return (
      <div className="min-h-screen flex items-center justify-center p-4">
        <Card className="w-full max-w-md">
          <CardContent className="pt-6 text-center">
            <div className="text-red-500 mb-4">加载失败</div>
            <p className="text-muted-foreground mb-4">{(error as Error).message}</p>
            <div className="space-x-2">
              <Button onClick={() => refetch()}>重试</Button>
              <Button variant="outline" onClick={handleLogout}>重新登录</Button>
            </div>
          </CardContent>
        </Card>
      </div>
    )
  }

  return (
    <div className="min-h-screen">
      {/* 顶部导航 */}
      <header className="sticky top-0 z-50 w-full border-b border-border/60 bg-background/80 backdrop-blur">
        <div className="container flex h-16 items-center justify-between px-4 md:px-8">
          <div className="flex items-center gap-2">
            <div className="flex h-9 w-9 items-center justify-center rounded-xl bg-primary/10 text-primary">
              <Server className="h-5 w-5" />
            </div>
            <div>
              <p className="text-xs uppercase tracking-[0.2em] text-muted-foreground">Kiro Admin</p>
              <span className="font-display text-base">凭据控制台</span>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <Button variant="ghost" size="icon" onClick={toggleDarkMode}>
              {darkMode ? <Sun className="h-5 w-5" /> : <Moon className="h-5 w-5" />}
            </Button>
            <Button variant="ghost" size="icon" onClick={handleRefresh}>
              <RefreshCw className="h-5 w-5" />
            </Button>
            <Button variant="ghost" size="icon" onClick={handleLogout}>
              <LogOut className="h-5 w-5" />
            </Button>
          </div>
        </div>
      </header>

      {/* 主内容 */}
      <main className="container px-4 md:px-8 py-8 space-y-8">
        <section className="rounded-3xl border border-border/60 bg-card/70 p-6 shadow-sm backdrop-blur animate-fade-up">
          <div className="flex flex-col gap-6 lg:flex-row lg:items-center lg:justify-between">
            <div className="max-w-xl">
              <p className="text-xs uppercase tracking-[0.3em] text-muted-foreground">
                Admin Overview
              </p>
              <h1 className="mt-2 text-3xl md:text-4xl font-display">
                凭据控制台
              </h1>
              <p className="mt-2 text-sm text-muted-foreground">
                统一管理 Token、优先级与代理设置，实时追踪活跃凭据与额度状态。
              </p>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <Button onClick={() => setAddDialogOpen(true)}>
                <Plus className="h-4 w-4 mr-2" />
                添加凭据
              </Button>
              <Button variant="outline" onClick={handleRefresh}>
                <RefreshCw className="h-4 w-4 mr-2" />
                刷新列表
              </Button>
            </div>
          </div>

          <div className="mt-6 grid gap-4 md:grid-cols-3 stagger-children">
            <div className="rounded-2xl border border-border/60 bg-background/80 p-4">
              <p className="text-xs uppercase tracking-[0.2em] text-muted-foreground">
                凭据总数
              </p>
              <div className="mt-3 text-3xl font-display">{data?.total || 0}</div>
            </div>
            <div className="rounded-2xl border border-border/60 bg-background/80 p-4">
              <p className="text-xs uppercase tracking-[0.2em] text-muted-foreground">
                可用凭据
              </p>
              <div className="mt-3 text-3xl font-display text-emerald-600">
                {data?.available || 0}
              </div>
            </div>
            <div className="rounded-2xl border border-border/60 bg-background/80 p-4">
              <p className="text-xs uppercase tracking-[0.2em] text-muted-foreground">
                当前活跃
              </p>
              <div className="mt-3 flex items-center gap-2 text-2xl font-display">
                #{data?.currentId || '-'}
                <Badge variant="success">活跃</Badge>
              </div>
            </div>
          </div>
        </section>

        <section className="space-y-4 animate-fade-up">
          <div className="flex flex-wrap items-center justify-between gap-4">
            <div>
              <h2 className="text-xl font-display">凭据管理</h2>
              <p className="text-sm text-muted-foreground">
                支持快速启停、优先级调整与代理配置。
              </p>
            </div>
          </div>
          {data?.credentials.length === 0 ? (
            <Card className="border-dashed">
              <CardContent className="py-10 text-center text-muted-foreground">
                暂无凭据，点击右上角添加新的凭据。
              </CardContent>
            </Card>
          ) : (
            <CredentialTable
              credentials={data?.credentials ?? []}
              onViewBalance={handleViewBalance}
            />
          )}
        </section>
      </main>

      {/* 余额对话框 */}
      <BalanceDialog
        credentialId={selectedCredentialId}
        open={balanceDialogOpen}
        onOpenChange={setBalanceDialogOpen}
      />

      {/* 添加凭据对话框 */}
      <AddCredentialDialog
        open={addDialogOpen}
        onOpenChange={setAddDialogOpen}
      />
    </div>
  )
}
