import { useEffect, useState } from 'react'
import { toast } from 'sonner'
import {
  Check,
  ChevronDown,
  ChevronUp,
  MoreHorizontal,
  RefreshCw,
  Trash2,
  Wallet,
  X,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import type { CredentialStatusItem, BalanceResponse } from '@/types/api'
import {
  useDeleteCredential,
  useResetFailure,
  useSetDisabled,
  useSetPriority,
  useSetProxy,
} from '@/hooks/use-credentials'
import { getCredentialBalance } from '@/api/credentials'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Input } from '@/components/ui/input'
import { Switch } from '@/components/ui/switch'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { Progress } from '@/components/ui/progress'

interface CredentialTableProps {
  credentials: CredentialStatusItem[]
  onViewBalance: (id: number) => void
}

function formatAuthMethodLabel(authMethod: string | null): string {
  if (!authMethod) return '未知'
  if (authMethod.toLowerCase() === 'idc') return 'IdC/Builder-ID/IAM'
  return authMethod
}

function formatExpiry(expiresAt: string | null): { label: string; tone: string } {
  if (!expiresAt) return { label: '未知', tone: 'text-muted-foreground' }
  const date = new Date(expiresAt)
  const diff = date.getTime() - Date.now()
  if (diff <= 0) return { label: '已过期', tone: 'text-red-600 font-semibold' }
  const minutes = Math.floor(diff / 60000)
  if (minutes < 60) {
    return {
      label: `${minutes} 分钟`,
      tone: minutes < 15 ? 'text-amber-600 font-semibold' : 'text-emerald-600 font-semibold',
    }
  }
  const hours = Math.floor(minutes / 60)
  if (hours < 24) {
    return { label: `${hours} 小时`, tone: 'text-emerald-600 font-semibold' }
  }
  return { label: `${Math.floor(hours / 24)} 天`, tone: 'text-emerald-600 font-semibold' }
}

interface CredentialRowProps {
  credential: CredentialStatusItem
  onViewBalance: (id: number) => void
}

function CredentialRow({ credential, onViewBalance }: CredentialRowProps) {
  const [editingPriority, setEditingPriority] = useState(false)
  const [priorityValue, setPriorityValue] = useState(String(credential.priority))
  const [editingProxy, setEditingProxy] = useState(false)
  const [proxyValue, setProxyValue] = useState(credential.proxyUrl || '')
  const [showDeleteDialog, setShowDeleteDialog] = useState(false)
  const [balance, setBalance] = useState<BalanceResponse | null>(null)
  const [balanceLoading, setBalanceLoading] = useState(true)

  const setDisabled = useSetDisabled()
  const setPriority = useSetPriority()
  const setProxy = useSetProxy()
  const resetFailure = useResetFailure()
  const deleteCredential = useDeleteCredential()

  // 获取余额信息
  useEffect(() => {
    let cancelled = false
    const fetchBalance = async () => {
      if (credential.disabled) {
        setBalance(null)
        setBalanceLoading(false)
        return
      }
      try {
        setBalanceLoading(true)
        const data = await getCredentialBalance(credential.id)
        if (!cancelled) {
          setBalance(data)
        }
      } catch {
        if (!cancelled) {
          setBalance(null)
        }
      } finally {
        if (!cancelled) {
          setBalanceLoading(false)
        }
      }
    }
    fetchBalance()
    // 每 60 秒刷新一次余额
    const interval = setInterval(fetchBalance, 60000)
    return () => {
      cancelled = true
      clearInterval(interval)
    }
  }, [credential.id, credential.disabled])

  useEffect(() => {
    if (!editingPriority) {
      setPriorityValue(String(credential.priority))
    }
  }, [credential.priority, editingPriority])

  useEffect(() => {
    if (!editingProxy) {
      setProxyValue(credential.proxyUrl || '')
    }
  }, [credential.proxyUrl, editingProxy])

  const handleToggleDisabled = () => {
    setDisabled.mutate(
      { id: credential.id, disabled: !credential.disabled },
      {
        onSuccess: (res) => {
          toast.success(res.message)
        },
        onError: (err) => {
          toast.error('操作失败: ' + (err as Error).message)
        },
      }
    )
  }

  const handlePriorityChange = () => {
    const newPriority = parseInt(priorityValue, 10)
    if (isNaN(newPriority) || newPriority < 0) {
      toast.error('优先级必须是非负整数')
      return
    }
    setPriority.mutate(
      { id: credential.id, priority: newPriority },
      {
        onSuccess: (res) => {
          toast.success(res.message)
          setEditingPriority(false)
        },
        onError: (err) => {
          toast.error('操作失败: ' + (err as Error).message)
        },
      }
    )
  }

  const handleProxyChange = () => {
    const trimmedProxy = proxyValue.trim()
    setProxy.mutate(
      { id: credential.id, proxyUrl: trimmedProxy || null },
      {
        onSuccess: (res) => {
          toast.success(res.message)
          setEditingProxy(false)
        },
        onError: (err) => {
          toast.error('操作失败: ' + (err as Error).message)
        },
      }
    )
  }

  const handleReset = () => {
    resetFailure.mutate(credential.id, {
      onSuccess: (res) => {
        toast.success(res.message)
      },
      onError: (err) => {
        toast.error('操作失败: ' + (err as Error).message)
      },
    })
  }

  const handleDelete = () => {
    deleteCredential.mutate(credential.id, {
      onSuccess: (res) => {
        toast.success(res.message)
        setShowDeleteDialog(false)
      },
      onError: (err) => {
        toast.error('删除失败: ' + (err as Error).message)
      },
    })
  }

  const expiry = formatExpiry(credential.expiresAt)

  return (
    <>
      <TableRow
        className={cn(
          credential.isCurrent && 'bg-primary/5',
          credential.disabled && 'opacity-80'
        )}
      >
        <TableCell className="min-w-[220px]">
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-primary/10 text-primary font-display text-sm">
              #{credential.id}
            </div>
            <div>
              <div className="flex flex-wrap items-center gap-2">
                <span className="font-semibold">凭据 #{credential.id}</span>
                {credential.isCurrent && <Badge variant="success">当前</Badge>}
                {credential.disabled && <Badge variant="destructive">已禁用</Badge>}
              </div>
              <div className="mt-1 text-xs text-muted-foreground">
                {formatAuthMethodLabel(credential.authMethod)}
                {credential.hasProfileArn && (
                  <span className="ml-2 rounded-full bg-secondary px-2 py-0.5 text-[10px] text-secondary-foreground">
                    Profile ARN
                  </span>
                )}
              </div>
            </div>
          </div>
        </TableCell>
        <TableCell className="min-w-[200px]">
          {credential.email ? (
            <span className="text-sm">{credential.email}</span>
          ) : (
            <span className="text-sm text-muted-foreground">—</span>
          )}
        </TableCell>
        <TableCell>
          <div className="flex items-center gap-2">
            <Switch
              checked={!credential.disabled}
              onCheckedChange={handleToggleDisabled}
              disabled={setDisabled.isPending}
            />
            <span className="text-xs text-muted-foreground">
              {credential.disabled ? '停用' : '启用'}
            </span>
          </div>
        </TableCell>
        <TableCell className="min-w-[150px]">
          {editingPriority ? (
            <div className="flex items-center gap-2">
              <Input
                type="number"
                value={priorityValue}
                onChange={(e) => setPriorityValue(e.target.value)}
                className="h-8 w-20 text-sm"
                min="0"
              />
              <Button
                size="icon"
                variant="ghost"
                className="h-8 w-8"
                onClick={handlePriorityChange}
                disabled={setPriority.isPending}
              >
                <Check className="h-4 w-4" />
              </Button>
              <Button
                size="icon"
                variant="ghost"
                className="h-8 w-8"
                onClick={() => {
                  setEditingPriority(false)
                  setPriorityValue(String(credential.priority))
                }}
              >
                <X className="h-4 w-4" />
              </Button>
            </div>
          ) : (
            <button
              type="button"
              className="text-sm font-semibold text-left hover:text-primary"
              onClick={() => setEditingPriority(true)}
            >
              {credential.priority}
              <span className="ml-2 text-xs font-normal text-muted-foreground">
                编辑
              </span>
            </button>
          )}
        </TableCell>
        <TableCell>
          <span
            className={cn(
              'text-sm font-semibold',
              credential.failureCount > 0 ? 'text-red-600' : 'text-muted-foreground'
            )}
          >
            {credential.failureCount}
          </span>
        </TableCell>
        <TableCell className="text-center">
          <span className="text-sm font-semibold text-blue-600">
            {credential.dailyCount}
          </span>
        </TableCell>
        <TableCell className="text-center">
          <span className="text-sm font-semibold text-emerald-600">
            {credential.totalCount}
          </span>
        </TableCell>
        <TableCell className="min-w-[180px]">
          {credential.disabled ? (
            <span className="text-xs text-muted-foreground">已禁用</span>
          ) : balanceLoading ? (
            <div className="flex items-center gap-2">
              <div className="animate-spin rounded-full h-3 w-3 border-b border-primary"></div>
              <span className="text-xs text-muted-foreground">加载中</span>
            </div>
          ) : balance ? (
            <div className="space-y-1">
              <div className="flex justify-between text-xs">
                <span className="text-muted-foreground">${balance.currentUsage.toFixed(0)}</span>
                <span className="text-muted-foreground">${balance.usageLimit.toFixed(0)}</span>
              </div>
              <Progress value={balance.usagePercentage} className="h-2" />
              <div className="text-xs text-center text-muted-foreground">
                剩余 ${balance.remaining.toFixed(0)}
              </div>
            </div>
          ) : (
            <span className="text-xs text-muted-foreground">无法获取</span>
          )}
        </TableCell>
        <TableCell>
          <span className={cn('text-sm', expiry.tone)}>{expiry.label}</span>
        </TableCell>
        <TableCell className="min-w-[200px]">
          {editingProxy ? (
            <div className="flex items-center gap-2">
              <Input
                type="text"
                value={proxyValue}
                onChange={(e) => setProxyValue(e.target.value)}
                className="h-8 text-sm"
                placeholder="例如 socks5://user:pass@host:port"
              />
              <Button
                size="icon"
                variant="ghost"
                className="h-8 w-8"
                onClick={handleProxyChange}
                disabled={setProxy.isPending}
              >
                <Check className="h-4 w-4" />
              </Button>
              <Button
                size="icon"
                variant="ghost"
                className="h-8 w-8"
                onClick={() => {
                  setEditingProxy(false)
                  setProxyValue(credential.proxyUrl || '')
                }}
              >
                <X className="h-4 w-4" />
              </Button>
            </div>
          ) : (
            <button
              type="button"
              className="text-left text-xs text-muted-foreground hover:text-primary"
              onClick={() => setEditingProxy(true)}
              title={credential.proxyUrl || '使用全局代理'}
            >
              {credential.proxyUrl ? (
                <span className="block truncate max-w-[240px]">{credential.proxyUrl}</span>
              ) : (
                <span>使用全局代理</span>
              )}
            </button>
          )}
        </TableCell>
        <TableCell className="text-right">
          <div className="flex items-center justify-end gap-2">
            <Button
              size="icon"
              variant="ghost"
              className="h-9 w-9"
              onClick={() => onViewBalance(credential.id)}
              title="查看余额"
            >
              <Wallet className="h-4 w-4" />
            </Button>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button size="icon" variant="ghost" className="h-9 w-9" title="更多操作">
                  <MoreHorizontal className="h-4 w-4" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-48">
                <DropdownMenuItem
                  onClick={handleReset}
                  disabled={resetFailure.isPending || credential.failureCount === 0}
                >
                  <RefreshCw className="mr-2 h-4 w-4" />
                  重置失败
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem
                  onClick={() => {
                    const newPriority = Math.max(0, credential.priority - 1)
                    setPriority.mutate(
                      { id: credential.id, priority: newPriority },
                      {
                        onSuccess: (res) => toast.success(res.message),
                        onError: (err) => toast.error('操作失败: ' + (err as Error).message),
                      }
                    )
                  }}
                  disabled={setPriority.isPending || credential.priority === 0}
                >
                  <ChevronUp className="mr-2 h-4 w-4" />
                  提高优先级
                </DropdownMenuItem>
                <DropdownMenuItem
                  onClick={() => {
                    const newPriority = credential.priority + 1
                    setPriority.mutate(
                      { id: credential.id, priority: newPriority },
                      {
                        onSuccess: (res) => toast.success(res.message),
                        onError: (err) => toast.error('操作失败: ' + (err as Error).message),
                      }
                    )
                  }}
                  disabled={setPriority.isPending}
                >
                  <ChevronDown className="mr-2 h-4 w-4" />
                  降低优先级
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem
                  destructive
                  onClick={() => setShowDeleteDialog(true)}
                  disabled={!credential.disabled}
                >
                  <Trash2 className="mr-2 h-4 w-4" />
                  删除
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </TableCell>
      </TableRow>

      <Dialog open={showDeleteDialog} onOpenChange={setShowDeleteDialog}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>确认删除凭据</DialogTitle>
            <DialogDescription>
              您确定要删除凭据 #{credential.id} 吗？此操作无法撤销。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setShowDeleteDialog(false)}
              disabled={deleteCredential.isPending}
            >
              取消
            </Button>
            <Button
              variant="destructive"
              onClick={handleDelete}
              disabled={deleteCredential.isPending}
            >
              确认删除
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}

export function CredentialTable({ credentials, onViewBalance }: CredentialTableProps) {
  return (
    <div className="rounded-2xl border border-border/60 bg-card/80 shadow-sm backdrop-blur">
      <Table className="min-w-[1100px]">
        <TableHeader>
          <TableRow>
            <TableHead>凭据</TableHead>
            <TableHead>邮箱</TableHead>
            <TableHead>状态</TableHead>
            <TableHead>优先级</TableHead>
            <TableHead>失败</TableHead>
            <TableHead className="text-center">
              <div>今日</div>
              <div className="text-xs text-muted-foreground">次数</div>
            </TableHead>
            <TableHead className="text-center">
              <div>总计</div>
              <div className="text-xs text-muted-foreground">次数</div>
            </TableHead>
            <TableHead className="min-w-[180px]">额度</TableHead>
            <TableHead>Token</TableHead>
            <TableHead>代理</TableHead>
            <TableHead className="text-right">操作</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {credentials.map((credential) => (
            <CredentialRow
              key={credential.id}
              credential={credential}
              onViewBalance={onViewBalance}
            />
          ))}
        </TableBody>
      </Table>
    </div>
  )
}
