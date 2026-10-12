# S002:建桶道 garage admin API 契约与业界建桶动词先例

> 吸收研究格 grok 单(2026-10-12,s5 bucket create 设计前置),契约
> 钉死 garage tag v1.1.0 源码;六态:已验证(本环境写入回环坐实)、
> 已观察(读到原文或只读响应)、推断(由已观察推出)、假设(无直接
> 证据)、待验(留后续)、已否证。garagehq 现行站描述 v2 API,本后端
> v1.1.0 只服务 v1(router_v2.rs 同 tag 404),v2 文档不适用 [已观察]。

## 背景

REQ-0002 全开放共享模型要 `s5 bucket create <名>`(garage admin 道,
建桶即授权当前 key)。需定:动词形态、重名语义、输出 ack、admin API
请求与错误契约、自动授权的先例。

## 关键结论

1. 动词形态:业界两族,短动词 `mb`(s3cmd/mc/s5cmd/aws s3)与显式
   `bucket create`(aws s3api create-bucket 族);garage 自家 CLI 即
   `garage bucket create <名>`,s5 取同形 [已观察]。
2. 重名默认失败:S3 CreateBucket 409(BucketAlreadyExists/
   BucketAlreadyOwnedByYou);幂等成功是少数且各有条件(mc 需显式
   `--ignore-existing`,az CLI 默认幂等但 created:false 语义混)。
   s5 对齐默认失败 [已观察]。
3. garage v1.1.0 admin 契约 [已观察,源码钉死]:
   - `POST /v1/bucket`:body camelCase,`globalAlias`(集群全局名,
     S3 寻址用)与 `localAlias` 都可选;成功 200 出 BucketInfo
     (id 为 64 hex,keys 空:只建 global alias 不授权任何 key)。
   - 重名:409,body 四字段 `{"code":"BucketAlreadyExists",
     "message":"Bucket already exists","region":..,"path":..}`;
     非法名 400 `InvalidRequest`。code 是字符串(OpenAPI yaml 列的
     错误码表是早期未测全面,以源码为准)。
   - `POST /v1/bucket/allow`:body `{bucketId, accessKeyId,
     permissions:{read,write,owner}}`;语义"true 打开,false 保持
     原样不关闭"(关闭走 deny),重复 allow 收敛。成功 200 出
     BucketInfo;桶/key 不存在 404 NoSuchBucket/NoSuchAccessKey。
   - `owner` 旗标在 v1.1.0 指改 website 配置与删桶,非 S3 ACL owner。
4. 自动授权先例:garage 生态标准拆法是建桶与授权两步(quick start
   `garage bucket allow --read --write`,Terraform 分资源),无隐含
   授权;S3 CreateBucket 自助路径在 v1.1.0 只设 local alias 不设
   global alias 且要求 key 的 allow_create_bucket,不能当共享桶实现
   [已观察,否证 S3 道替代 admin 道]。
5. 两步缝:create 成功而 allow 失败时,重试 create 得 409 不补权限;
   补救是 `GET /v1/bucket?globalAlias=` 取 id 再 allow [推断,源码
   语义推出]。

## 现状与实测

- s5 实装:bucketchan 走 POST /v1/bucket(globalAlias)加
  POST /v1/bucket/allow(read+write+true,owner 不给:桶生命周期归
  总台,agent 不删桶),后端字符串 code 原码透传。
- 本工位 wire 实测(2026-10-12):假 bearer 打 3903 得 403,信封
  AccessDenied,消息取自服务端体 [已验证];200 BucketInfo 与 409
  形状源码已核 [已观察];admin token 配发后真回环补跑已毕:双面
  (wsl 加 lan-ubuntu)`bucket create` 建桶、授权即写读、门控测试
  `bucket_create_grants_and_roundtrip_on_garage` 全过 [已验证]。
- 补实(2026-10-12):DELETE /v1/bucket 对空桶 204;`delete-objects`
  参数 v1.1.0 不认(非空桶三形皆 409 BucketNotEmpty),清桶须先删
  对象 [已验证];CLI 面同理(`garage bucket delete --yes` 无清对象
  旗标)。

## 踩坑沉淀

- admin 面错误体 code 是字符串不是数字;OpenAPI yaml 的错误码表
  未测全,契约以源码为准。
- garage CLI `bucket create` 重名报 BadRequest 文本,不走 admin HTTP
  409;CLI 与 admin HTTP 是两条道,别混引。
- v1.1.0 无 v2 admin 面;看现行 garagehq 文档会拿错契约。

## 待办

- (已闭,2026-10-12)admin token 双面配发与真回环:门控测试全过,
  409 形状与清桶路径坐实。

## 信源

- garage v1.1.0 源码(tag 钉死):src/api/admin/{router_v1,bucket,
  error,key}.rs、src/api/common/{common_error,helpers}.rs、
  src/api/s3/bucket.rs、src/model/{permission,bucket_alias_table}.rs、
  src/garage/{admin,cli}/bucket.rs、doc/api/garage-admin-v1.yml、
  doc/book/quick-start(git.deuxfleurs.fr,同内容 GitHub 镜像
  deuxfleurs-org/garage blob v1.1.0)
- aws s3api create-bucket 与 S3 API CreateBucket 文档;mc mb 与
  mc-admin policy attach(源码 v2.3.0/mb-main.go);s5cmd mb(v2.3.0);
  s3cmd(v2.4.0);rclone s3 provider 文档(v1.68.0);az storage
  container create(文档加 issue 25868)
- 授权先例:garagehq S3 compatibility 页;terraform-provider-garage
  (jkossis)与 registry.terraform.io Arsolitt/garagehq 指南
