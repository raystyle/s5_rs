# S001:文件道与流道后端选型,s2-lite 面与 garage 边界

> 立项研究档:吸收总台研究格 research-s5-01(2026-10-11,238 行,信源为上游
> 文档、仓原文与对 192.168.88.2 只读探测)加本工位实测补强。六态:已验证
> (本环境写入回环坐实)、已观察(读到原文或只读响应)、推断(由已观察
> 推出)、假设(无直接证据)、待验(留后续)、已否证。

## 背景

s5 立项(s5-01):Rust 封装 agent 跨机共享 CLI,S3 文件道 + S2 流道,零感知
凭据。两道选型与后端边界需要一份可引的档,避免每个工位重研。

## 关键结论

1. 文件道选 `object_store` 的 aws feature[已验证]:path-style 是默认(IP
   端点必需),`with_allow_http(true)` 放行明文端点,region 必须等于服务端
   `s3_region`(garage 缺省 `garage`,SigV4 scope 敏感,实测 us-east-1 报
   AuthorizationHeaderMalformed、garage 即通)。put/get/ls/del 四动词与
   trait 对齐,翻页内建,不拉 smithy 生成面;与 s2-sdk 同 tokio 运行时。
2. `aws-sdk-s3` 不选 [已观察]:能连 garage,但编译面最大,且默认校验和
   (CRC32、aws-chunked、trailer)在 S3 兼容实现上反复出事;garage v1.1.0
   只有 trailer 流式签名,没有 2.x 的 checksum-type,不当默认。
3. 流道用官方 `s2-sdk` [已验证]:tokio,lite 兼容 v0.22+;端点经
   `S2Endpoints::for_endpoint` 单 URL 双用(lite 无子域路由,basin 由 SDK
   经 `S2-Basin` 头承载,缺头 400 bad_header 实测复现)。
4. s2 协议是 REST(`/v1` 前缀 JSON),不是独立 gRPC 服务 [已观察];流式读
   是同一 GET 的会话形(SDK 用的 S2S 帧 + SSE 备选),Protobuf 只是内容
   类型。s2-lite 无鉴权面(`S2_ACCESS_TOKEN=ignored` 官方口径;本机 8090
   无 Authorization 得 200 实测),与零感知凭据设计相合。
5. 流读语义 [已验证]:裸 GET(无起点)等于停在 tail 上,返回 416 加
   `{"tail":{...}}`;读最后 N 条必须 `tail_offset`;`seq_num` 起点含本号;
   越尾起点是「游标在尾上」非故障(s5 tail --seq 越尾退 0 空出,实测)。
6. `tail` 默认有界、`-f` 才跟随 [已观察]:unix tail / kubectl logs / heroku
   logs / s2 tail 自家 CLI 的共同交点;agent 一次调用要在有限时间返回。
7. garage v1.1.0 边界 [已观察]:无版本化(删即真删)、无 Put 条件写
   (If-Match 只在 Get/Head)、小文件单次 Put 不上分片、ListObjectsV2 已
   实现(一页 1000,前缀是字面字节非 glob)。

## 现状或实测

本工位 2026-10-11 实测(全部已验证):

- garage <http://192.168.88.2:3900>(dev key,GKdb…)对 s5-dev 桶
  PUT/GET/LS/DEL 回环通;region=garage 必需;403 XML 错误体含
  `<Code>` 可抽语义码(NoSuchKey/AccessDenied 实测)。
- s2-lite 容器 @ wrt :8090(GET /health 回 OK,唯一 basin rx-home-basin,
  内含 secret-guard-log-v2..v5 与 test 流,勿动);basin 未开
  `create_stream_on_append`(对不存在的流 append 报 stream_not_found,
  需先 create stream,s5 的自动建流由此来)。
- 本地起 `s2 lite`(官方 s2-cli v0.43.1 musl,SHA256SUMS 校验)在
  127.0.0.1:7171,建 basin `s5-dev-basin`:append(自动建流)/tail
  缺省/-n/--seq 续读/越尾空出/streams 列表/tail -f(SIGINT 退 130)全链
  回环通,即 tests/cli.rs 的流道门控件。
- basin 名至少 8 字节(s5-dev 6 字节被官方 CLI 拒,实测)。

## 踩坑沉淀

- 非 ASCII 键:object_store 对键做 URL 编码后发请求,garage 把编码后的
  字面串当键存 [已验证,aws cli 对照];s5 内部 put/get/del 同串往返一致,
  跨工具(aws cli)见编码形。首版不解码、建议 agent 用 ASCII 键。
- object_store 0.13 起便利动词(put/get/delete)在 `ObjectStoreExt` 扩展
  trait,核心 trait 只留 `*_opts`;import 少了 Ext 会报「方法不存在」。
- s2-sdk 名字类型(BasinName/StreamName)是 newtype,`.parse()` 需显式
  标类型;错误面 `ServerError{code,message}` 加 `ErrorCode` 枚举
  (strum snake_case Display),`ReadError::ReadUnwritten` 是 416 的
  类型化形态,tail 越尾空出走它。
- `SequencedRecord::from_parts(seq, ts, headers, body)` 参数序与字段序
  不同(headers 在前)。

## 待办

- [ ] 生产桶 herdr-flywheel 接入(总台管 key,首版后)。
- [ ] 大文件分片上传(若工件超过单次 Put 量级再上,garage 分片端点官方
  表 Implemented 但算法细节未核对)。
- [ ] wrt s2-lite 容器 env 只读核对(docker inspect 看 AWS_ENDPOINT_URL_S3
  与桶名,确认 SlateDB 落在本网 garage;本轮守只读边界未读容器内)。
